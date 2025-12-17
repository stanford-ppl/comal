#![allow(dead_code)]

use std::{fs, str::FromStr, time::Instant};

use comal::{
    cli_common::{DamOptions, SamOptionFiles},
    proto_driver::{build_from_proto, proto_headers::tortilla::ComalGraph, Channels},
    templates::primitive::{Repsiggen, Token},
};

use clap::Parser;
use dam::{
    simulation::ProgramBuilder,
    utility_contexts::{ConsumerContext, GeneratorContext},
};
use prost::Message;

use crate::channel_file::{ChannelFile, ChannelType};

mod channel_file;

#[derive(Parser, Debug)]
struct Cli {
    /// Protobuffer containing a tortilla graph
    #[arg(long)]
    proto: String,

    /// Data directory for the graph
    #[arg(long)]
    data: String,

    /// Print timing breakdowns
    #[arg(long)]
    breakdowns: bool,

    /// Channel Data Files
    #[arg(short, long)]
    channel: Vec<String>,

    #[command(flatten)]
    dam_opts: DamOptions,

    #[command(flatten)]
    sam_opts: SamOptionFiles,
}

fn main() {
    let start = Instant::now();
    let args = Cli::parse();
    let comal_graph = {
        let file_contents = fs::read(&args.proto).unwrap();
        ComalGraph::decode(file_contents.as_slice()).unwrap()
    };

    let mut repsig = Channels::default();
    let mut vals = Channels::default();
    let mut coords = Channels::default();
    let mut refs = Channels::default();
    let mut builder = ProgramBuilder::default();

    // populate the channel structures
    args.channel.iter().for_each(|channel_file| {
        let toml_string = std::fs::read_to_string(channel_file.as_str()).unwrap();
        let toml_file: ChannelFile = toml::from_str(toml_string.as_str()).unwrap();
        let id = toml_file.id as u64;
        let channel_type = ChannelType::from_str(toml_file.tp.as_str()).unwrap();
        match channel_type {
            ChannelType::Value => {
                let (snd, rcv) = builder.unbounded();
                builder.add_child(GeneratorContext::new(
                    move || {
                        toml_file
                            .parse_payload(|s| {
                                Token::try_from(s)
                                    .unwrap_or_else(|_| Token::Val(f32::from_str(s).unwrap()))
                            })
                            .into_iter()
                    },
                    snd,
                ));
                vals.set_receiver(id, rcv);
            }
            ChannelType::Coordinate => {
                let (snd, rcv) = builder.unbounded();
                builder.add_child(GeneratorContext::new(
                    move || {
                        toml_file
                            .parse_payload(|s| {
                                Token::try_from(s)
                                    .unwrap_or_else(|_| Token::Val(u32::from_str(s).unwrap()))
                            })
                            .into_iter()
                    },
                    snd,
                ));
                coords.set_receiver(id, rcv);
            }
            ChannelType::Reference => {
                let (snd, rcv) = builder.unbounded();
                builder.add_child(GeneratorContext::new(
                    move || {
                        toml_file
                            .parse_payload(|s| {
                                Token::try_from(s)
                                    .unwrap_or_else(|_| Token::Val(u32::from_str(s).unwrap()))
                            })
                            .into_iter()
                    },
                    snd,
                ));
                refs.set_receiver(id, rcv);
            }
            ChannelType::Repeat => {
                let (snd, rcv) = builder.unbounded();
                builder.add_child(GeneratorContext::new(
                    move || {
                        toml_file
                            .parse_payload(|s| Repsiggen::try_from(s).unwrap())
                            .into_iter()
                    },
                    snd,
                ));
                repsig.set_receiver(id, rcv);
            }
        }
    });

    build_from_proto(
        comal_graph,
        args.data.into(),
        (&args.sam_opts).into(),
        &mut builder,
        &mut refs,
        &mut coords,
        &mut vals,
        &mut repsig,
    );

    refs.iter_remainders()
        .for_each(|remainder| builder.add_child(ConsumerContext::new(remainder)));

    coords
        .iter_remainders()
        .for_each(|remainder| builder.add_child(ConsumerContext::new(remainder)));

    vals.iter_remainders()
        .for_each(|remainder| builder.add_child(ConsumerContext::new(remainder)));

    repsig
        .iter_remainders()
        .for_each(|remainder| builder.add_child(ConsumerContext::new(remainder)));

    let end_parse = Instant::now();
    if args.breakdowns {
        println!("Parse Time: {:?}", end_parse - start);
    }
    let initialized = builder.initialize(args.dam_opts.into()).unwrap();
    let initialized_time = Instant::now();
    if args.breakdowns {
        println!("Initialization Time: {:?}", initialized_time - end_parse);
    }

    let executed = initialized.run(args.dam_opts.into());
    if args.breakdowns {
        println!("Execution Time: {:?}", initialized_time.elapsed());
    }
    println!("Elapsed Cycles: {}", executed.elapsed_cycles().unwrap());
}
