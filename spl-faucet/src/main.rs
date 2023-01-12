use std::env;

use anyhow::Result;
use clap::Parser;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    instruction::{AccountMeta, Instruction},
    program_pack::Pack,
    pubkey::Pubkey,
    signature::{read_keypair_file, Keypair},
    signer::Signer,
    system_instruction::create_account,
    sysvar,
    transaction::Transaction,
};
use spl_token::{solana_program::pubkey, state::Mint};
use spl_token_faucet::instruction::FaucetInstruction;

#[tokio::main]
async fn main() {
    let Opts { global, command } = Opts::parse();

    let rpc = RpcClient::new(global.url);
    let payer = read_keypair_file(shellexpand::tilde(&global.wallet).to_string())
        .expect("failed to read keypair");

    match command {
        Command::Create {
            max_amount,
            decimals,
        } => inti_mint_and_faucet(decimals, payer, rpc, max_amount),
        Command::Airdrop { .. } | Command::Close { .. } => todo!(),
    }
    .await
    .unwrap();
}

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const FAUCET_PROGRAM_ID: Pubkey = pubkey!("4bXpkKSV8swHSnwqtzuboGPaPDeEgAn4Vt8GfarV5rZt");

#[derive(Debug, Parser)]
struct Opts {
    #[clap(flatten)]
    pub global: GlobalOpts,
    #[clap(subcommand)]
    pub command: Command,
}

#[derive(Debug, Parser)]
struct GlobalOpts {
    #[clap(
        global = true,
        short,
        long,
        default_value = "https://api.mainnet-beta.solana.com/"
    )]
    pub url: String,
    #[clap(
        global = true,
        short = 'k',
        long = "keypair",
        default_value = "~/.config/solana/id.json"
    )]
    pub wallet: String,
}

#[derive(Debug, Parser)]
#[clap(version = VERSION)]
enum Command {
    Create {
        #[clap(short, long)]
        max_amount: u64,
        #[clap(short, long)]
        decimals: u8,
    },
    Airdrop {
        #[clap(short, long)]
        faucet: String,
        #[clap(short, long)]
        amount: u64,
    },
    Close {
        #[clap(short, long)]
        faucet: String,
    },
}

async fn inti_mint_and_faucet(
    decimals: u8,
    payer: Keypair,
    rpc: RpcClient,
    ui_amount: u64,
) -> Result<()> {
    let mint_keypair = Keypair::new();
    let faucet_keypair = Keypair::new();
    let mint_authority = get_faucet_pda().0;

    let amount = ui_amount * 10u64.pow(decimals as u32);

    let tx = Transaction::new_signed_with_payer(
        &[
            create_account(
                &payer.pubkey(),
                &mint_keypair.pubkey(),
                rpc.get_minimum_balance_for_rent_exemption(Mint::LEN)
                    .await?,
                Mint::LEN as u64,
                &spl_token::ID,
            ),
            spl_token::instruction::initialize_mint2(
                &spl_token::ID,
                &mint_keypair.pubkey(),
                &mint_authority,
                None,
                decimals,
            )?,
            create_account(
                &payer.pubkey(),
                &faucet_keypair.pubkey(),
                rpc.get_minimum_balance_for_rent_exemption(spl_token_faucet::state::Faucet::LEN)
                    .await?,
                spl_token_faucet::state::Faucet::LEN as u64,
                &FAUCET_PROGRAM_ID,
            ),
            create_init_faucet_ix(mint_keypair.pubkey(), faucet_keypair.pubkey(), None, amount),
        ],
        Some(&payer.pubkey()),
        &[&payer, &mint_keypair, &faucet_keypair],
        rpc.get_latest_blockhash().await?,
    );

    let sig = rpc
        .send_and_confirm_transaction_with_spinner_and_commitment(
            &tx,
            CommitmentConfig::confirmed(),
        )
        .await?;

    println!("Transaction signature: {}", sig);

    println!(
        "Mint: {}\nFaucet: {}",
        mint_keypair.pubkey(),
        faucet_keypair.pubkey()
    );

    Ok(())
}

fn get_faucet_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"faucet"], &FAUCET_PROGRAM_ID)
}

fn create_init_faucet_ix(
    mint_account: Pubkey,
    faucet_account: Pubkey,
    admin: Option<Pubkey>,
    amount: u64,
) -> Instruction {
    let mut accounts = vec![
        AccountMeta::new_readonly(mint_account, false),
        AccountMeta::new(faucet_account, false),
        AccountMeta::new_readonly(sysvar::rent::id(), false),
    ];

    if let Some(admin) = admin {
        accounts.push(AccountMeta::new_readonly(admin, false));
    }

    Instruction {
        program_id: FAUCET_PROGRAM_ID,
        accounts,
        data: FaucetInstruction::InitFaucet { amount }.pack(),
    }
}
