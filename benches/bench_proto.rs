use std::{fs, path::Path};

use comal::{
    config::Data,
    proto_driver::{parse_proto, proto_headers::tortilla::ComalGraph},
};
use criterion::{criterion_group, criterion_main, BenchmarkGroup, BenchmarkId, Criterion};
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
                        let mut parent = parse_proto(comal_graph.clone(), base_path.clone());
                        parent.set_inference(*flavor);
                        parent.init();
                        parent
                    },
                    |mut parent| {
                        parent.run();
                        parent
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

criterion_group!(sam_benches, bench_sddmm,);
criterion_main!(sam_benches);
