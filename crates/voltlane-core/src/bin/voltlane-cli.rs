use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use voltlane_core::{
    diagnostics::init_tracing,
    export::{export_midi, export_mp3, export_wav},
    fixtures::demo_project,
    generate_parity_report,
    parity::write_parity_report,
    persistence::save_project,
};

#[derive(Debug, Parser)]
#[command(name = "voltlane-cli")]
#[command(about = "Headless tools for Voltlane project/export/parity workflows")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(long, default_value = "logs")]
    log_dir: PathBuf,
}

#[derive(Debug, Subcommand)]
enum Commands {
    DemoExport {
        #[arg(long, default_value = "data/exports")]
        output_dir: PathBuf,

        #[arg(long, value_enum, default_value = "all")]
        format: DemoFormat,
    },
    ParityReport {
        #[arg(long, default_value = "data/parity/report.json")]
        output: PathBuf,
    },
}

#[derive(Debug, Clone, ValueEnum)]
enum DemoFormat {
    Midi,
    Wav,
    Mp3,
    All,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let _telemetry = init_tracing(&cli.log_dir)?;

    match cli.command {
        Commands::DemoExport { output_dir, format } => {
            std::fs::create_dir_all(&output_dir)?;
            let project = demo_project();
            save_project(&output_dir.join("demo.voltlane.json"), &project)?;

            match format {
                DemoFormat::Midi => export_midi(&project, &output_dir.join("demo.mid"))?,
                DemoFormat::Wav => export_wav(&project, &output_dir.join("demo.wav"))?,
                DemoFormat::Mp3 => export_mp3(&project, &output_dir.join("demo.mp3"), None)?,
                DemoFormat::All => {
                    export_midi(&project, &output_dir.join("demo.mid"))?;
                    export_wav(&project, &output_dir.join("demo.wav"))?;
                    if let Err(error) = export_mp3(&project, &output_dir.join("demo.mp3"), None) {
                        tracing::warn!(?error, "mp3 export skipped because ffmpeg is unavailable");
                    }
                }
            }
        }
        Commands::ParityReport { output } => {
            let report = generate_parity_report(&demo_project())?;
            write_parity_report(&output, &report)?;
            tracing::info!(path = %output.display(), "parity report generated");
        }
    }

    Ok(())
}
