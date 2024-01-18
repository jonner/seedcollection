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
    #[command(about = "Manage seed projects")]
    Project {
        #[command(subcommand)]
        command: ProjectCommands,
    },
    #[command(about = "Manage sources")]
    Source {
        #[command(subcommand)]
        command: SourceCommands,
    },
    #[command(about = "Manage samples")]
    Sample {
        #[command(subcommand)]
        command: SampleCommands,
    },
    #[command(about = "Manage users")]
    User {
        #[command(subcommand)]
        command: UserCommands,
    },
}

#[derive(Subcommand, Debug)]
pub enum ProjectCommands {
    #[command(about = "List all projects")]
    List {
        #[arg(short, long)]
        full: bool,
    },
    #[command(about = "Add a new project to the database")]
    Add {
        #[arg(short, long)]
        name: String,
        #[arg(short, long)]
        description: Option<String>,
        #[arg(short, long)]
        userid: i64,
    },
    #[command(
        about="Modify properties of a project",
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
    #[command(about = "Remove a project from the database")]
    Remove { id: i64 },
    #[command(about = "Add a new sample to the project")]
    AddSample {
        #[arg(short, long)]
        project: i64,
        #[arg(short, long)]
        sample: i64,
    },
    #[command(about = "Remove an existing sample from the project")]
    RemoveSample {
        #[arg(short, long)]
        project: i64,
        #[arg(short, long)]
        sample: i64,
    },
    #[command(about = "Show all details about a project")]
    Show {
        id: i64,
        #[arg(short, long)]
        full: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum SourceCommands {
    #[command(about = "List all sources")]
    List {
        #[arg(short, long)]
        full: bool,
    },
    #[command(about = "Add a new source to the database")]
    Add {
        #[arg(long)]
        name: String,
        #[arg(long)]
        description: Option<String>,
        #[arg(long = "lat")]
        latitude: Option<f64>,
        #[arg(long = "long")]
        longitude: Option<f64>,
        #[arg(long)]
        userid: Option<i64>,
    },
    #[command(about = "Remove an existing source from the database")]
    Remove { id: i64 },
    #[command(
        about="Modify properties of a source",
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
        source: Option<i64>,
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
                .args(&["taxon", "source", "month", "year", "quantity", "notes"]),
        ),
        about="Modify properties of a sample")]
    Modify {
        #[arg(long)]
        id: i64,
        #[arg(long)]
        taxon: Option<i64>,
        #[arg(long)]
        source: Option<i64>,
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

#[derive(Subcommand, Debug)]
pub enum UserCommands {
    #[command(about = "List all users")]
    List {},
    #[command(about = "Add a new user to the database")]
    Add {
        #[arg(long)]
        username: String,
        #[arg(long)]
        passwordfile: Option<PathBuf>,
    },
    #[command(about = "Remove an existing user from the database")]
    Remove { id: i64 },
    #[command(
        about="Modify properties about a user",
        group(
            clap::ArgGroup::new("modify")
                .required(true)
                .multiple(true)
                .args(&["username", "change_password"]),
        ))]
    Modify {
        #[arg(long)]
        id: i64,
        #[arg(long)]
        username: Option<String>,
        #[arg(long, short = 'p')]
        change_password: bool,
        #[arg(long, short = 'f', requires("change_password"))]
        password_file: Option<PathBuf>,
    },
}
