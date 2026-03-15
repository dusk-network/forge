use crate::error::{CliError, Result};

use super::embedded::{TemplateKind, files};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractName {
    pub kebab: String,
    pub module: String,
    pub pascal: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedTemplate {
    pub cargo_toml: String,
    pub lib_rs: String,
    pub test_rs: String,
    pub rust_toolchain_toml: String,
    pub gitignore: String,
    pub makefile: String,
}

pub fn validate_contract_name(name: &str) -> Result<ContractName> {
    if name.is_empty() {
        return Err(CliError::InvalidContractName {
            name: name.to_string(),
            reason: "name cannot be empty".to_string(),
        });
    }

    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(CliError::InvalidContractName {
            name: name.to_string(),
            reason: "use lowercase letters, digits, and hyphens only".to_string(),
        });
    }

    if !name.chars().next().is_some_and(|c| c.is_ascii_lowercase()) {
        return Err(CliError::InvalidContractName {
            name: name.to_string(),
            reason: "name must start with a lowercase letter".to_string(),
        });
    }

    if name.ends_with('-') {
        return Err(CliError::InvalidContractName {
            name: name.to_string(),
            reason: "name cannot end with a hyphen".to_string(),
        });
    }

    if name.contains("--") {
        return Err(CliError::InvalidContractName {
            name: name.to_string(),
            reason: "name cannot contain consecutive hyphens".to_string(),
        });
    }

    let module = name.replace('-', "_");
    let pascal = name
        .split('-')
        .filter(|segment| !segment.is_empty())
        .map(to_pascal_segment)
        .collect::<Vec<_>>()
        .join("");

    Ok(ContractName {
        kebab: name.to_string(),
        module,
        pascal,
    })
}

pub fn render_template(template: TemplateKind, name: &ContractName) -> RenderedTemplate {
    let template = files(template);

    RenderedTemplate {
        cargo_toml: apply_common_replacements(template.cargo_toml, name),
        lib_rs: apply_common_replacements(template.lib_rs, name),
        test_rs: apply_test_replacements(template.test_rs, name),
        rust_toolchain_toml: template.rust_toolchain_toml.to_string(),
        gitignore: template.gitignore.to_string(),
        makefile: template.makefile.to_string(),
    }
}

fn apply_test_replacements(content: &str, name: &ContractName) -> String {
    let with_wasm_name =
        content.replace("YOUR_CONTRACT_NAME.wasm", &format!("{}.wasm", name.module));
    apply_common_replacements(&with_wasm_name, name)
}

fn apply_common_replacements(content: &str, name: &ContractName) -> String {
    let intermediate = content
        .replace("YOUR_CONTRACT_NAME", &name.kebab)
        .replace("YOUR_MODULE_NAME", &name.module)
        .replace("YOUR_STRUCT_NAME", &name.pascal)
        .replace("mod counter", &format!("mod {}", name.module));

    replace_identifier(&intermediate, "Counter", &name.pascal)
}

fn replace_identifier(content: &str, from: &str, to: &str) -> String {
    let mut result = String::with_capacity(content.len());
    let bytes = content.as_bytes();
    let from_bytes = from.as_bytes();

    if from_bytes.is_empty() || bytes.len() < from_bytes.len() {
        return content.to_string();
    }

    let mut i = 0;
    while i + from_bytes.len() <= bytes.len() {
        if let Some(relative_pos) = content[i..].find(from) {
            let absolute_pos = i + relative_pos;
            let before_is_ident = absolute_pos > 0 && is_identifier_byte(bytes[absolute_pos - 1]);
            let after_index = absolute_pos + from_bytes.len();
            let after_is_ident =
                after_index < bytes.len() && is_identifier_byte(bytes[after_index]);

            if before_is_ident || after_is_ident {
                result.push_str(&content[i..=absolute_pos]);
                i = absolute_pos + 1;
                continue;
            }

            result.push_str(&content[i..absolute_pos]);
            result.push_str(to);
            i = after_index;
        } else {
            break;
        }
    }

    result.push_str(&content[i..]);
    result
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn to_pascal_segment(segment: &str) -> String {
    let mut chars = segment.chars();
    match chars.next() {
        Some(first) => {
            let mut out = String::new();
            out.push(first.to_ascii_uppercase());
            out.extend(chars);
            out
        }
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_kebab_case_name() {
        let parsed = validate_contract_name("my-counter").expect("name should be valid");
        assert_eq!(parsed.kebab, "my-counter");
        assert_eq!(parsed.module, "my_counter");
        assert_eq!(parsed.pascal, "MyCounter");
    }

    #[test]
    fn rejects_invalid_characters() {
        let err = validate_contract_name("MyCounter").expect_err("name should be invalid");
        assert!(err.to_string().contains("lowercase letters"));
    }

    #[test]
    fn renders_counter_template_replacements() {
        let name = validate_contract_name("bridge-test").expect("valid");
        let rendered = render_template(TemplateKind::Counter, &name);

        assert!(rendered.cargo_toml.contains("name = \"bridge-test\""));
        assert!(rendered.lib_rs.contains("mod bridge_test"));
        assert!(rendered.lib_rs.contains("pub struct BridgeTest"));
        assert!(rendered.test_rs.contains("release/bridge_test.wasm"));
        assert!(!rendered.test_rs.contains("YOUR_CONTRACT_NAME"));
    }

    #[test]
    fn renders_empty_template_without_counter_struct() {
        let name = validate_contract_name("empty-app").expect("valid");
        let rendered = render_template(TemplateKind::Empty, &name);

        assert!(rendered.lib_rs.contains("mod empty_app"));
        assert!(rendered.lib_rs.contains("pub struct EmptyApp"));
        assert!(!rendered.lib_rs.contains("CountChanged"));
    }

    #[test]
    fn replace_identifier_only_replaces_standalone_identifiers() {
        let rendered = replace_identifier(
            "Counter Encounter Counter_ Counter::TOPIC",
            "Counter",
            "BridgeTest",
        );

        assert_eq!(rendered, "BridgeTest Encounter Counter_ BridgeTest::TOPIC");
    }
}
