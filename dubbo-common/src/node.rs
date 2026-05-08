use crate::url::URL;

pub trait Node: Send + Sync {
    fn get_url(&self) -> &URL;

    fn is_available(&self) -> bool;

    fn destroy(&self);
}
