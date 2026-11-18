use std::process::Command;

#[test]
fn running_cli_prints_hello_inifra() {
    let output = Command::new(env!("CARGO_BIN_EXE_inifra"))
        .output()
        .expect("run Inifra");

    assert!(output.status.success());
    assert!(output.stdout == b"Hello, Inifra!\n" || output.stdout == b"Hello, Inifra!\r\n");
    assert!(output.stderr.is_empty());
}
