#![allow(clippy::unwrap_used)]

use super::*;
use syn::{Attribute, DeriveInput};

#[test]
fn test_attribute() {
    let attribute = create_attribute(
        r#"
            #[scope(title = "Read Repository", parent = "Repository")]
            "#,
    );
    let result = attribute.parse_args::<ScopeAttribute>().unwrap();
    assert_eq!(result.title.value(), "Read Repository");
    assert_eq!(result.parent.unwrap().value(), "Repository");
}

fn create_attribute(attribute: &str) -> Attribute {
    let actual_input = format!(
        r#"
            {attribute}
            struct Test;
            "#
    );
    let input = syn::parse_str::<DeriveInput>(&actual_input).unwrap();
    let attributes = input.attrs;
    attributes[0].clone()
}
