#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]

#[test]
pub fn test() {
    let from = super::from_string("2024-08-28T00:09:11.230Z").unwrap();
    println!("{:?}", from);
}
