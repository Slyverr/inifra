use std::process::{Command, Output};

use tempfile::tempdir;

fn generate(template: &str, inputs: &[&str]) -> (tempfile::TempDir, Output) {
    let config = tempdir().expect("create isolated config directory");
    let project = tempdir().expect("create project directory");
    let mut arguments = vec![
        "generate".to_owned(),
        template.to_owned(),
        "--path".to_owned(),
        project.path().to_string_lossy().into_owned(),
    ];
    for input in inputs {
        arguments.push("--set".to_owned());
        arguments.push((*input).to_owned());
    }

    let mut command = Command::new(env!("CARGO_BIN_EXE_inifra"));
    command
        .args(arguments)
        .env("INIFRA_CONFIG_DIR", config.path());
    let output = command.output().expect("run Inifra");
    (project, output)
}

#[test]
fn guidance_and_documentation_templates_render_project_name() {
    for (template, path, heading) in [
        ("agents", "AGENTS.md", "# Agent Instructions — Example"),
        (
            "docs-architecture",
            "docs/architecture.md",
            "# Architecture — Example",
        ),
        (
            "docs-development",
            "docs/development.md",
            "# Development — Example",
        ),
    ] {
        let (project, output) = generate(template, &["project_name=Example"]);
        assert!(
            output.status.success(),
            "generation failed for {template}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let contents = std::fs::read_to_string(project.path().join(path))
            .unwrap_or_else(|error| panic!("could not read {path}: {error}"));
        assert!(contents.starts_with(heading));
        assert!(!contents.contains("{{ project_name }}"));
    }
}

#[test]
fn github_templates_emit_issue_forms_and_pull_request_template() {
    let (project, issues) = generate("github-issues", &[]);
    assert!(
        issues.status.success(),
        "issue form generation failed: {}",
        String::from_utf8_lossy(&issues.stderr)
    );
    assert!(
        project
            .path()
            .join(".github/ISSUE_TEMPLATE/bug-report.yml")
            .is_file()
    );
    assert!(
        project
            .path()
            .join(".github/ISSUE_TEMPLATE/feature-request.yml")
            .is_file()
    );

    let (project, pull_request) = generate("github-pull-request", &[]);
    assert!(
        pull_request.status.success(),
        "pull request template generation failed: {}",
        String::from_utf8_lossy(&pull_request.stderr)
    );
    assert!(
        project
            .path()
            .join(".github/pull_request_template.md")
            .is_file()
    );
}

#[test]
fn security_template_requires_and_renders_reporting_details() {
    let (project, output) = generate(
        "security",
        &[
            "project_name=Example",
            "supported_versions=1.x receives fixes",
            "security_contact=https://example.invalid/security-advisories",
        ],
    );
    assert!(
        output.status.success(),
        "security template generation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let policy = std::fs::read_to_string(project.path().join("SECURITY.md"))
        .expect("read generated security policy");
    assert!(policy.contains("1.x receives fixes"));
    assert!(policy.contains("https://example.invalid/security-advisories"));
    assert!(!policy.contains("{{ security_contact }}"));
}
