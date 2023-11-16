use std::{fs, path::Path};

use comal::{
    config::Data,
    proto_driver::{parse_proto, proto_headers::tortilla::ComalGraph},
};
use criterion::{criterion_group, criterion_main, BenchmarkGroup, BenchmarkId, Criterion};
use dam::simulation::{InitializationOptionsBuilder, RunMode, RunOptionsBuilder};
use prost::Message;

fn bench_proto<M: criterion::measurement::Measurement>(
    bench_group: &mut BenchmarkGroup<M>,
    data_dir_name: String,
    proto_filename: String,
) {
    let config_file = home::home_dir().unwrap().join("sam_config.toml");
    let contents = fs::read_to_string(config_file).unwrap();
    let data: Data = toml::from_str(&contents).unwrap();
    let formatted_dir = data.sam_config.sam_path;
    let base_path = Path::new(&formatted_dir).join(&data_dir_name);

    let comal_contents = fs::read(base_path.join(proto_filename)).unwrap();
    let comal_graph = ComalGraph::decode(comal_contents.as_slice()).unwrap();

    for with_flavor in [true, false] {
        bench_group.bench_with_input(
            BenchmarkId::from_parameter(with_flavor),
            &with_flavor,
            |b, flavor| {
                b.iter_batched(
                    || {
                        let parent = parse_proto(comal_graph.clone(), base_path.clone());
                        // parent.set_inference(*flavor);
                        // parent.init();
                        // parent
                        parent
                            .initialize(
                                InitializationOptionsBuilder::default()
                                    .run_flavor_inference(*flavor)
                                    .build()
                                    .unwrap(),
                            )
                            .unwrap()
                    },
                    |parent| {
                        parent.run(
                            RunOptionsBuilder::default()
                                .mode(RunMode::FIFO)
                                .build()
                                .unwrap(),
                        );
                    },
                    criterion::BatchSize::LargeInput,
                );
            },
        );
    }
}

fn bench_proto_sweep<M: criterion::measurement::Measurement>(
    bench_group: &mut BenchmarkGroup<M>,
    dir_lst: Vec<&str>,
    proto_filename: String,
) {
    let config_file = home::home_dir().unwrap().join("sam_config.toml");
    let contents = fs::read_to_string(config_file).unwrap();
    let data: Data = toml::from_str(&contents).unwrap();
    let formatted_dir = data.sam_config.sam_path;

    for dir_name in dir_lst {
        bench_group.bench_with_input(
            BenchmarkId::from_parameter(dir_name),
            &dir_name,
            |b, dir| {
                b.iter_batched(
                    || {
                        let base_path = Path::new(&formatted_dir).join(*dir);
                        let comal_contents =
                            fs::read(base_path.join(proto_filename.clone())).unwrap();
                        let comal_graph = ComalGraph::decode(comal_contents.as_slice()).unwrap();
                        let parent = parse_proto(comal_graph.clone(), base_path.clone());
                        parent
                            .initialize(
                                InitializationOptionsBuilder::default()
                                    .run_flavor_inference(true)
                                    .build()
                                    .unwrap(),
                            )
                            .unwrap()
                    },
                    |parent| {
                        parent.run(
                            RunOptionsBuilder::default()
                                .mode(RunMode::FIFO)
                                .build()
                                .unwrap(),
                        );
                    },
                    criterion::BatchSize::LargeInput,
                );
            },
        );
    }
}

pub fn bench_sddmm(c: &mut Criterion) {
    let mut group = c.benchmark_group("SDDMM");
    let data_dir_name = "sddmm".to_string();
    let proto_filename = "sddmm.bin".to_string();
    bench_proto(&mut group, data_dir_name, proto_filename);
    group.finish();
}

pub fn bench_sddmm_sweep(c: &mut Criterion) {
    let mut group = c.benchmark_group("SDDMM");
    let proto_filename = "sddmm.bin".to_string();
    let dir_lst = vec![
        "sddmm_100",
        "sddmm_200",
        "sddmm_300",
        "sddmm_400",
        "sddmm_500",
    ];
    bench_proto_sweep(&mut group, dir_lst, proto_filename);
    group.finish();
}

pub fn bench_add_sweep(c: &mut Criterion) {
    let mut group = c.benchmark_group("add");
    group.sample_size(10);
    let proto_filename = "matadd.bin".to_string();
    let dir_lst = vec![
        "matadd_1000",
        "matadd_2000",
        "matadd_3000",
        "matadd_4000",
        "matadd_5000",
    ];
    bench_proto_sweep(&mut group, dir_lst, proto_filename);
    group.finish();
}

pub fn bench_matmul_sweep(c: &mut Criterion) {
    let mut group = c.benchmark_group("matmul");
    let proto_filename = "matmul.bin".to_string();
    let dir_lst = vec![
        "matmul_100",
        "matmul_200",
        "matmul_300",
        // "matmul_400",
        // "matmul_500",
    ];
    bench_proto_sweep(&mut group, dir_lst, proto_filename);
    group.finish();
}

criterion_group!(sam_benches, bench_add_sweep,);
criterion_main!(sam_benches);
