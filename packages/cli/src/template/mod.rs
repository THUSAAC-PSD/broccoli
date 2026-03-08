pub mod variables;

use std::fs;
use std::path::Path;

use anyhow::Result;

use self::variables::TemplateVars;

/// Replace all `{{var_name}}` placeholders in a template string.
pub fn render(template: &str, vars: &TemplateVars) -> String {
    template
        .replace("{{plugin_name}}", &vars.plugin_name)
        .replace("{{plugin_name_snake}}", &vars.plugin_name_snake)
        .replace("{{plugin_name_pascal}}", &vars.plugin_name_pascal)
        .replace("{{server_sdk_dep}}", &vars.server_sdk_dep)
        .replace("{{web_sdk_dep}}", &vars.web_sdk_dep)
        .replace("{{web_root}}", &vars.web_root)
}

/// Render a template and write it to the given output path, creating parent
/// directories as needed.
pub fn write_template(template: &str, output: &Path, vars: &TemplateVars) -> Result<()> {
    let content = render(template, vars);
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(output, content)?;
    Ok(())
}
