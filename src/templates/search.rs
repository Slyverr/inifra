use super::Template;

pub(crate) struct TemplateQuery {
    terms: Vec<String>,
}

impl TemplateQuery {
    pub(crate) fn new(query: &str) -> Self {
        Self {
            terms: query.split_whitespace().map(str::to_lowercase).collect(),
        }
    }

    pub(crate) fn matches(&self, template: &Template, category: Option<&str>) -> bool {
        if self.terms.is_empty() {
            return true;
        }

        let id = template.id.to_lowercase();
        let description = template.description.to_lowercase();
        let category = category.unwrap_or_default().to_lowercase();
        let paths = template
            .files
            .iter()
            .map(|file| file.path.to_string_lossy().to_lowercase())
            .collect::<Vec<_>>();

        self.terms.iter().all(|term| {
            id.contains(term)
                || description.contains(term)
                || category.contains(term)
                || paths.iter().any(|path| path.contains(term))
        })
    }
}
