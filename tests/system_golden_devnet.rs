//! System test scaffold for local devnet.
//! Run manually with:
//!   cargo test --test system_golden_devnet -- --ignored --nocapture

#[test]
#[ignore = "requires running local devnet + sn-api-server"]
fn system_golden_devnet_binary_exists() {
    let p = std::path::Path::new("target/debug/examples/golden_devnet");
    assert!(
        p.exists(),
        "build example first: cargo build --example golden_devnet"
    );
}
