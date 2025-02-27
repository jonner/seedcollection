use tabled::Table;

pub(crate) trait SeedctlTable {
    fn styled(&mut self) -> &mut Self;
}

impl SeedctlTable for Table {
    fn styled(&mut self) -> &mut Self {
        use tabled::settings::{Modify, Style, object::Segment, width::Width};
        let m = Modify::new(Segment::all()).with(Width::wrap(60).keep_words(true));
        self.with(m).with(Style::psql())
    }
}
