use std::borrow::Cow;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassDef<'a> {
    pub class_name: Cow<'a, str>,
    pub field_names: Vec<Cow<'a, str>>,
}
