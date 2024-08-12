use tabled::Table;

pub trait SeedctlTable {
    fn styled(&mut self) -> &mut Self;
}

impl SeedctlTable for Table {
    fn styled(&mut self) -> &mut Self {
        use tabled::settings::{object::Segment, width::Width, Modify, Style};
        let m = Modify::new(Segment::all()).with(Width::wrap(60).keep_words(true));
        self.with(m).with(Style::psql())
    }
}
