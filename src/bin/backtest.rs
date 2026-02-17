use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "backtest")]
#[command(about = "Polymarket HFT Backtesting Engine", long_about = None)]
struct Args {
    /// Strategy to backtest: spread_farming, weather, copy_trade, lp, penny_longshot, custom_bot
    #[arg(short, long)]
    strategy: String,

    /// Start date (YYYY-MM-DD)
    #[arg(short, long)]
    start: String,

    /// End date (YYYY-MM-DD)
    #[arg(short, long)]
    end: String,

    /// Path to historical data directory
    #[arg(short, long, default_value = "tests/fixtures")]
    data_path: PathBuf,

    /// Output report path
    #[arg(short, long, default_value = "backtest_report.csv")]
    output: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    println!("🔍 Backtesting Strategy: {}", args.strategy);
    println!("📅 Period: {} to {}", args.start, args.end);
    println!("📊 Data: {}", args.data_path.display());
    println!("💾 Output: {}", args.output.display());

    // TODO: Phase 8 will implement actual backtest engine
    println!("\n⏳ Backtest engine coming in Phase 8...");

    Ok(())
}
