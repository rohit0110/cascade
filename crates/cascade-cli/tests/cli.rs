use std::fs;
use std::path::Path;
use std::process::Command;

fn cascade(args: &[&str]) -> (i32, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_cascade")).args(args).output().unwrap();
    (out.status.code().unwrap(), String::from_utf8_lossy(&out.stdout).into_owned())
}

fn setup(dir: &Path) {
    fs::write(
        dir.join("checkout.yaml"),
        "service: checkout\nowner: pay\nfeatures:\n  - {id: checkout, tier: P0, depends_on: [{service: payment}, {service: recs, kind: soft}]}\n",
    )
    .unwrap();
    fs::write(dir.join("payment.yaml"), "service: payment\nowner: pay\n").unwrap();
}

fn tmp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("cascade-cli-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn verify_passes_then_fails_on_new_dependency() {
    let d = tmp("verify");
    setup(&d);
    let obs = d.join("obs.json");
    let edge = |c: &str, s: &str| format!(r#"{{"client":"{c}","server":"{s}","rps":1.0,"error_ratio":0.0}}"#);
    fs::write(&obs, format!("[{},{}]", edge("checkout", "payment"), edge("checkout", "recs"))).unwrap();
    let (code, out) = cascade(&["verify", "--registry", d.to_str().unwrap(), "--observed", obs.to_str().unwrap()]);
    assert_eq!(code, 0, "{out}");

    fs::write(&obs, format!("[{},{},{}]", edge("checkout", "payment"), edge("checkout", "recs"), edge("checkout", "fraud"))).unwrap();
    let (code, out) = cascade(&["verify", "--registry", d.to_str().unwrap(), "--observed", obs.to_str().unwrap()]);
    assert_eq!(code, 1);
    assert!(out.contains("undeclared dependency checkout -> fraud"), "{out}");
}

#[test]
fn verify_strict_fails_on_unobserved() {
    let d = tmp("strict");
    setup(&d);
    let obs = d.join("obs.json");
    fs::write(&obs, r#"[{"client":"checkout","server":"payment","rps":1.0,"error_ratio":0.0}]"#).unwrap();
    let args = ["verify", "--registry", d.to_str().unwrap(), "--observed", obs.to_str().unwrap()];
    assert_eq!(cascade(&args).0, 0);
    let mut strict = args.to_vec();
    strict.push("--strict");
    assert_eq!(cascade(&strict).0, 1);
}

#[test]
fn simulate_reports_soft_as_degraded() {
    let d = tmp("sim");
    setup(&d);
    let r = d.to_str().unwrap();
    let (code, out) = cascade(&["simulate", "--registry", r, "--down", "payment"]);
    assert_eq!(code, 0);
    assert!(out.contains("down") && out.contains("checkout"), "{out}");
    let (_, out) = cascade(&["simulate", "--registry", r, "--down", "recs"]);
    assert!(out.contains("degraded"), "{out}");
    assert_eq!(cascade(&["simulate", "--registry", r, "--down", "ghost"]).0, 2);
}

#[test]
fn init_writes_loadable_starter_files() {
    let d = tmp("init");
    let obs = d.join("obs.json");
    fs::write(&obs, r#"[{"client":"a","server":"b","rps":1.0,"error_ratio":0.0},{"client":"a","server":"unknown","rps":1.0,"error_ratio":0.0}]"#).unwrap();
    let out_dir = d.join("reg");
    let (code, _) = cascade(&["init", "--out", out_dir.to_str().unwrap(), "--observed", obs.to_str().unwrap()]);
    assert_eq!(code, 0);
    let text = fs::read_to_string(out_dir.join("a.yaml")).unwrap();
    assert!(text.contains("service: b") && !text.contains("unknown"), "{text}");
    // Second run must not clobber.
    let (_, again) = cascade(&["init", "--out", out_dir.to_str().unwrap(), "--observed", obs.to_str().unwrap()]);
    assert!(again.contains("skip"), "{again}");
    // And it round-trips through the registry loader.
    let (code, _) = cascade(&["simulate", "--registry", out_dir.to_str().unwrap(), "--down", "b"]);
    assert_eq!(code, 0);
}
