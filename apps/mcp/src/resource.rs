use rmcp::{ErrorData, model::*};

const PRESETS: &[(&str, &str)] = &[
    (
        "default.toml",
        include_str!("../../../configs/default.toml"),
    ),
    ("rust.toml", include_str!("../../../configs/rust.toml")),
    ("c.toml", include_str!("../../../configs/c.toml")),
];

pub(crate) fn list() -> Vec<Resource> {
    let mut resources = vec![
        Resource::new(crate::skill::URI, crate::skill::NAME)
            .with_description("Software design workflow bundled with this server.")
            .with_mime_type("text/markdown")
            .with_size(crate::skill::TEXT.len() as u64),
    ];
    resources.extend(PRESETS.iter().map(|(name, text)| {
        Resource::new(format!("linter://configs/{name}"), *name)
            .with_description("Configuration preset to save as the project's linter.toml.")
            .with_mime_type("application/toml")
            .with_size(text.len() as u64)
    }));
    resources
}

pub(crate) fn read(uri: &str) -> Result<ReadResourceResult, ErrorData> {
    let (text, mime) = if uri == crate::skill::URI {
        (crate::skill::TEXT, "text/markdown")
    } else {
        let text = PRESETS
            .iter()
            .find(|(name, _)| uri == format!("linter://configs/{name}"))
            .map(|(_, text)| *text)
            .ok_or_else(|| ErrorData::resource_not_found("unknown resource", None))?;
        (text, "application/toml")
    };
    Ok(ReadResourceResult::new(vec![
        ResourceContents::text(text, uri).with_mime_type(mime),
    ]))
}
