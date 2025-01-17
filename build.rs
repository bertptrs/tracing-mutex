use autocfg::AutoCfg;

fn main() {
    // To avoid bumping MSRV unnecessarily, we can sniff certain features. Reevaluate this on major
    // releases.
    let ac = AutoCfg::new().unwrap();
    ac.emit_has_path("std::sync::LazyLock");

    autocfg::rerun_path("build.rs");
}
