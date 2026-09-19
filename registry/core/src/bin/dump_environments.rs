//! Prints this build's environment catalogue as canonical JSON.
//!
//! Used by `registry/conformance/check.sh`, which diffs this output against
//! `registry/conformance/environments.json` and against `ui-catalog`'s
//! equivalent dump. That is how the two hand-maintained lists are
//! kept from drifting.
//!
//! Only the *identity* slice is emitted — id, chain, environment, display
//! name, kind. Consensus constants are deliberately absent: they exist only in
//! this crate, because only the signer needs them, so there is nothing to
//! compare them against.
//!
//! The JSON is written by hand rather than serialised, to keep this crate free
//! of dependencies (see `Cargo.toml`). Every value is a known ASCII literal
//! from the catalogue, so there is nothing to escape.

use thadam_registry::{Environment, ENVIRONMENTS};

fn main() {
    let mut environments: Vec<&Environment> = ENVIRONMENTS.iter().collect();
    environments.sort_by_key(|e| e.id.to_string());

    println!("{{");
    println!("  \"environments\": [");
    for (index, environment) in environments.iter().enumerate() {
        let separator = if index + 1 == environments.len() {
            ""
        } else {
            ","
        };
        println!("    {{");
        println!("      \"id\": \"{}\",", environment.id);
        println!("      \"chain\": \"{}\",", environment.id.chain());
        println!(
            "      \"environment\": \"{}\",",
            environment.id.environment_name()
        );
        println!("      \"displayName\": \"{}\",", environment.display_name);
        println!("      \"kind\": \"{}\"", environment.kind);
        println!("    }}{separator}");
    }
    println!("  ]");
    println!("}}");
}
