// RGB node providing smart contracts functionality for Bitcoin & Lightning.
//
// Written in 2022 by
//     Dr. Maxim Orlovsky <orlovsky@lnp-bp.org>
//
// Copyright (C) 2022 by LNP/BP Standards Association, Switzerland.
//
// You should have received a copy of the MIT License along with this software.
// If not, see <https://opensource.org/licenses/MIT>.

#[macro_use]
extern crate clap;
#[macro_use]
extern crate amplify;

use std::fs;

use clap::IntoApp;
use clap_complete::generate_to;
use clap_complete::shells::*;

pub mod cli {
    include!("src/opts.rs");
}

fn main() -> Result<(), configure_me_codegen::Error> {
    let outdir = "../shell";

    fs::create_dir_all(outdir).expect("failed to create shell dir");
    for app in [cli::Opts::command()].iter_mut() {
        let name = app.get_name().to_string();
        generate_to(Bash, app, &name, &outdir)?;
        generate_to(PowerShell, app, &name, &outdir)?;
        generate_to(Zsh, app, &name, &outdir)?;
    }

    // configure_me_codegen::build_script_auto()
    Ok(())
}
