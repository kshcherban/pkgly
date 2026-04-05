#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;
#[test]
pub fn basic_test() {
    let route = FrontendRoute::try_from("/test/:id".to_string()).unwrap();
    println!("{:?}", route);
    assert_eq!(route.parts.len(), 2);
    assert_eq!(
        route.parts[0],
        FrontendRouteComponent::String("test".to_string())
    );
    assert_eq!(
        route.parts[1],
        FrontendRouteComponent::Param {
            key: "id".to_string(),
            optional: false,
            catch_all: false
        }
    );

    assert!(route.matches_path("/test/123"));
    assert!(route.matches_path("/test/123/"));

    assert!(!route.matches_path("/test/"));
}
#[test]
pub fn browse_test() {
    let route = FrontendRoute::try_from("/browse/:id/:catchAll(.*)?".to_string()).unwrap();
    println!("{:?}", route);
    assert_eq!(route.parts.len(), 3);
    assert_eq!(
        route.parts[0],
        FrontendRouteComponent::String("browse".to_string())
    );
    assert_eq!(
        route.parts[1],
        FrontendRouteComponent::Param {
            key: "id".to_string(),
            optional: false,
            catch_all: false
        }
    );
    assert_eq!(
        route.parts[2],
        FrontendRouteComponent::Param {
            key: "catchAll".to_string(),
            optional: true,
            catch_all: true
        }
    );
    assert!(route.matches_path("/browse/123/456"));
    assert!(route.matches_path("/browse/123/456/"));
    assert!(route.matches_path("/browse/123/456/789"));
    assert!(route.matches_path("/browse/123/456/789/"));
    assert!(!route.matches_path("/not_browse/"));
}

#[test]
fn parse_all() {
    let file = include_str!("../../../../../site/src/router/routes.json");
    let routes: Vec<RouteItem> = serde_json::from_str(file).unwrap();

    for route in routes {
        println!("{:?}", route);
    }
}
