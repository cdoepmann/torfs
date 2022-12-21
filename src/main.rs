use anyhow;
use tor_circuit_generator::CircuitGenerator;

mod cli;
use cli::Cli;

mod input;
use input::{ConsensusHandle, TorArchive};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let archive = TorArchive::new(cli.tor_data)?;

    let handles = archive.find_consensuses(cli.from, cli.to)?;

    for handle in handles {
        dbg!(&handle);
        let (consensus, descriptors) = handle.load()?;
        let circgen = CircuitGenerator::new(&consensus, descriptors, vec![443, 80, 22]);
        circgen
            .build_circuit(3, 443)
            .map_err(|_| anyhow::anyhow!("error building circuit"))?;
    }

    Ok(())
}
