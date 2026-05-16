use epaint::Pos2;

#[derive(Clone)]
pub struct Wall {
    pub(crate) start: Pos2,
    pub(crate) end: Pos2,
}
