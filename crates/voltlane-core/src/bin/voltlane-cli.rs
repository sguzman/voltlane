use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use voltlane_core::{
    RenderMode,
    diagnostics::init_tracing,
    export::{export_midi, export_mp3, export_stem_wav, export_wav},
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

        #[arg(long, value_enum, default_value = "offline")]
        render_mode: RenderModeArg,
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
    StemWav,
    All,
}

#[derive(Debug, Clone, ValueEnum)]
enum RenderModeArg {
    Offline,
    Realtime,
}

impl From<RenderModeArg> for RenderMode {
    fn from(value: RenderModeArg) -> Self {
        match value {
            RenderModeArg::Offline => Self::Offline,
            RenderModeArg::Realtime => Self::Realtime,
        }
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let _telemetry = init_tracing(&cli.log_dir)?;

    match cli.command {
        Commands::DemoExport {
            output_dir,
            format,
            render_mode,
        } => {
            std::fs::create_dir_all(&output_dir)?;
            let project = demo_project();
            save_project(&output_dir.join("demo.voltlane.json"), &project)?;
            let render_mode: RenderMode = render_mode.into();

            match format {
                DemoFormat::Midi => export_midi(&project, &output_dir.join("demo.mid"))?,
                DemoFormat::Wav => export_wav(&project, &output_dir.join("demo.wav"), render_mode)?,
                DemoFormat::Mp3 => {
                    export_mp3(&project, &output_dir.join("demo.mp3"), None, render_mode)?
                }
                DemoFormat::StemWav => {
                    let _paths = export_stem_wav(&project, &output_dir.join("stems"), render_mode)?;
                }
                DemoFormat::All => {
                    export_midi(&project, &output_dir.join("demo.mid"))?;
                    export_wav(&project, &output_dir.join("demo.wav"), render_mode)?;
                    let _paths = export_stem_wav(&project, &output_dir.join("stems"), render_mode)?;
                    if let Err(error) =
                        export_mp3(&project, &output_dir.join("demo.mp3"), None, render_mode)
                    {
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
