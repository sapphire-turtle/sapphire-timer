use std::net::SocketAddr;

use sapphire_timer::serve;

fn addr(s: &str) -> SocketAddr {
    s.parse().expect("addr")
}

#[test]
fn loopback_needs_no_allowed_host() {
    serve::check_exposure(addr("127.0.0.1:3174"), &[])
        .expect("a loopback bind is complete on its own");
}

#[test]
fn a_wide_bind_without_an_allowed_host_is_refused() {
    let err = serve::check_exposure(addr("0.0.0.0:3174"), &[])
        .expect_err("a wide bind with no allowlist serves 403 to everyone");
    let msg = err.to_string();
    assert!(
        msg.contains("allowed_hosts"),
        "the error must name the setting that fixes it, got: {msg}"
    );
}

#[test]
fn a_wide_bind_with_an_allowed_host_is_accepted() {
    serve::check_exposure(addr("0.0.0.0:3174"), &["tally.example.net".to_string()])
        .expect("a named host makes a wide bind usable");
}

#[test]
fn a_blank_allowed_host_does_not_count() {
    serve::check_exposure(addr("0.0.0.0:3174"), &["  ".to_string()])
        .expect_err("whitespace is not a hostname");
}

#[test]
fn the_device_config_is_checked_only_when_it_serves_mcp() {
    let wide = "[http]\nlisten = \"0.0.0.0:3175\"\n";
    serve::check_device_config(wide).expect("no /mcp, nothing to refuse");
    serve::check_device_config(&format!("{wide}[http.mcp]\nenabled = true\n"))
        .expect_err("/mcp on a wide bind with no allowlist is refused");
    serve::check_device_config(&format!(
        "{wide}allowed_hosts = [\"box.example.net\"]\n[http.mcp]\nenabled = true\n"
    ))
    .expect("a named host makes it usable");
    serve::check_device_config("[http.mcp]\nenabled = true\n")
        .expect("the default address is loopback");
}
