use super::catalog::{EmbeddedFile, EmbeddedPack};

pub(super) const PACKS: &[EmbeddedPack] = &[
    EmbeddedPack {
        manifest: include_str!("../../templates/changelog/template.toml"),
        files: &[EmbeddedFile {
            source: "CHANGELOG.md",
            contents: include_bytes!("../../templates/changelog/CHANGELOG.md"),
        }],
    },
    EmbeddedPack {
        manifest: include_str!("../../templates/editorconfig/template.toml"),
        files: &[EmbeddedFile {
            source: ".editorconfig",
            contents: include_bytes!("../../templates/editorconfig/.editorconfig"),
        }],
    },
    EmbeddedPack {
        manifest: include_str!("../../templates/gitignore-rust/template.toml"),
        files: &[EmbeddedFile {
            source: ".gitignore",
            contents: include_bytes!("../../templates/gitignore-rust/.gitignore"),
        }],
    },
];
