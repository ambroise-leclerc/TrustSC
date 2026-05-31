use mdux::{
    CompiledNode, CompiledNodeKind, CompiledScreenPackage, CriticalButtonSpec, CvCheckKind,
    GoldenReferenceEntry, LayoutKind, LayoutSpec, MduxResult, Rect, SystemEvent,
    ValidationError, ViewportReservation,
};

include!(concat!(env!("OUT_DIR"), "/hello_world_medui.rs"));

pub fn hello_world_screen_package() -> &'static CompiledScreenPackage {
    &GENERATED_MEDUI_PACKAGE
}

pub fn hello_world_primary_text_node() -> MduxResult<&'static CompiledNode> {
    hello_world_screen_package()
        .find_node(GENERATED_PRIMARY_TEXT_NODE_ID)
        .ok_or_else(|| ValidationError::new("generated MedUI package is missing its primary text node"))
}

pub fn hello_world_primary_text_key() -> MduxResult<&'static str> {
    hello_world_primary_text_node()?
        .kind
        .text_key()
        .ok_or_else(|| ValidationError::new("generated MedUI primary text node is missing a text key"))
}
