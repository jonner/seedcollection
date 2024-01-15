use crate::taxonomy;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Cli {
    #[arg(short, long, default_value = "seedcollection.sqlite")]
    pub database: PathBuf,
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    #[command(about = "Query taxonomy")]
    Taxonomy {
        #[command(subcommand)]
        command: TaxonomyCommands,
    },
    #[command(about = "Manage seed collections")]
    Collection {
        #[command(subcommand)]
        command: CollectionCommands,
    },
    #[command(about = "Manage locations")]
    Location {
        #[command(subcommand)]
        command: LocationCommands,
    },
    #[command(about = "Manage samples")]
    Sample {
        #[command(subcommand)]
        command: SampleCommands,
    },
}

#[derive(Subcommand, Debug)]
pub enum CollectionCommands {
    #[command(about = "List all collections")]
    List {
        #[arg(short, long)]
        full: bool,
    },
    #[command(about = "Add a new collection to the database")]
    Add {
        #[arg(short, long)]
        name: String,
        #[arg(short, long)]
        description: Option<String>,
        #[arg(short, long)]
        userid: i64,
    },
    #[command(
        about="Modify properties of a collection",
        group(
            clap::ArgGroup::new("modify")
                .required(true)
                .multiple(true)
                .args(&["name", "description"]),
        ))]
    Modify {
        #[arg(short, long)]
        id: i64,
        #[arg(short, long)]
        name: Option<String>,
        #[arg(short, long)]
        description: Option<String>,
    },
    #[command(about = "Remove a collection from the database")]
    Remove { id: i64 },
    #[command(about = "Add a new sample to the collection")]
    AddSample {
        #[arg(short, long)]
        collection: i64,
        #[arg(short, long)]
        sample: i64,
    },
    #[command(about = "Remove an existing sample from the collection")]
    RemoveSample {
        #[arg(short, long)]
        collection: i64,
        #[arg(short, long)]
        sample: i64,
    },
    #[command(about = "Show all details about a collection")]
    Show {
        id: i64,
        #[arg(short, long)]
        full: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum LocationCommands {
    #[command(about = "List all locations")]
    List {
        #[arg(short, long)]
        full: bool,
    },
    #[command(about = "Add a new location to the database")]
    Add {
        #[arg(long)]
        name: String,
        #[arg(long)]
        description: Option<String>,
        #[arg(long = "lat")]
        latitude: Option<f64>,
        #[arg(long = "long")]
        longitude: Option<f64>,
        #[arg(long = "long")]
        userid: Option<i64>,
    },
    #[command(about = "Remove an existing location from the database")]
    Remove { id: i64 },
    #[command(
        about="Modify properties about a location",
        group(
            clap::ArgGroup::new("modify")
                .required(true)
                .multiple(true)
                .args(&["name", "description", "latitude", "longitude"]),
        ))]
    Modify {
        #[arg(long)]
        id: i64,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long = "lat")]
        latitude: Option<f64>,
        #[arg(long = "long")]
        longitude: Option<f64>,
    },
}

#[derive(Subcommand, Debug)]
pub enum SampleCommands {
    #[command(about = "List all samples")]
    List {
        #[arg(short, long)]
        full: bool,
    },
    #[command(about = "Add a new sample to the database")]
    Add {
        #[arg(short, long)]
        taxon: i64,
        #[arg(short, long)]
        location: Option<i64>,
        #[arg(short, long)]
        month: Option<u16>,
        #[arg(short, long)]
        year: Option<u16>,
        #[arg(short, long)]
        quantity: Option<u32>,
        #[arg(short, long)]
        notes: Option<String>,
    },
    #[command(about = "Remove an existing sample from the database")]
    Remove { id: i64 },
    #[command(
        group(
            clap::ArgGroup::new("modify")
                .required(true)
                .multiple(true)
                .args(&["taxon", "location", "month", "year", "quantity", "notes"]),
        ),
        about="Modify properties of a sample")]
    Modify {
        #[arg(long)]
        id: i64,
        #[arg(long)]
        taxon: Option<i64>,
        #[arg(long)]
        location: Option<i64>,
        #[arg(short, long)]
        month: Option<u16>,
        #[arg(short, long)]
        year: Option<u16>,
        #[arg(short, long)]
        quantity: Option<u32>,
        #[arg(short, long)]
        notes: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum TaxonomyCommands {
    #[command(about = "Find a taxon in the database")]
    Find {
        #[arg(long, help = "Only show taxa with the given ID")]
        id: Option<i64>,
        #[arg(long, help = "Only show taxa with the given rank (e.g. 'family')")]
        rank: Option<taxonomy::Rank>,
        #[arg(long, help = "Only show taxa in the given genus")]
        genus: Option<String>,
        #[arg(long, help = "Only show taxa in the given species")]
        species: Option<String>,
        #[arg(long, help = "Show taxa with the given string in any field")]
        any: Option<String>,
        #[arg(long, help = "Show only taxa found in Minnesota")]
        minnesota: bool,
    },
}
