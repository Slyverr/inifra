pub(crate) struct Template<'a> {
    pub(crate) id: &'static str,
    pub(crate) description: &'static str,
    pub(crate) outputs: &'a [OutputFile],
}

pub(crate) struct OutputFile {
    pub(crate) path: &'static str,
    pub(crate) contents: &'static str,
}

const CHANGELOG_OUTPUTS: &[OutputFile] = &[OutputFile {
    path: "CHANGELOG.md",
    contents: include_str!("../templates/changelog/CHANGELOG.md"),
}];

const EDITORCONFIG_OUTPUTS: &[OutputFile] = &[OutputFile {
    path: ".editorconfig",
    contents: include_str!("../templates/editorconfig/.editorconfig"),
}];

const GITIGNORE_RUST_OUTPUTS: &[OutputFile] = &[OutputFile {
    path: ".gitignore",
    contents: include_str!("../templates/gitignore-rust/.gitignore"),
}];

const TEMPLATES: &[Template<'static>] = &[
    Template {
        id: "changelog",
        description: "Keep a record of notable project changes",
        outputs: CHANGELOG_OUTPUTS,
    },
    Template {
        id: "editorconfig",
        description: "Portable editor formatting defaults",
        outputs: EDITORCONFIG_OUTPUTS,
    },
    Template {
        id: "gitignore-rust",
        description: "Ignore Rust build output",
        outputs: GITIGNORE_RUST_OUTPUTS,
    },
];

pub(crate) fn all() -> &'static [Template<'static>] {
    TEMPLATES
}

pub(crate) fn find(id: &str) -> Option<&'static Template<'static>> {
    TEMPLATES.iter().find(|template| template.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_template_ids_are_unique() {
        let mut ids = std::collections::HashSet::new();

        for template in all() {
            assert!(
                ids.insert(template.id),
                "duplicate template ID: {}",
                template.id
            );
            assert!(
                !template.outputs.is_empty(),
                "empty template: {}",
                template.id
            );
        }
    }
}
