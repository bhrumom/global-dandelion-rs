use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use dandelion_core::{Chunk, MAX_PAYLOAD_BYTES};
fn packet(c: &mut Criterion) {
    let chunk = Chunk {
        transfer_id: [1; 16],
        file_id: 1,
        index: 0,
        last: false,
        payload: vec![42; MAX_PAYLOAD_BYTES],
    };
    let mut bytes = Vec::new();
    dandelion_packet::encode(&chunk, &mut bytes).unwrap();
    let mut group = c.benchmark_group("packet-v2");
    group.throughput(Throughput::Bytes(MAX_PAYLOAD_BYTES as u64));
    group.bench_function("encode-reuse", |b| {
        b.iter(|| dandelion_packet::encode(std::hint::black_box(&chunk), &mut bytes).unwrap())
    });
    group.bench_function("decode-verify", |b| {
        b.iter(|| dandelion_packet::decode(std::hint::black_box(&bytes)).unwrap())
    });
    group.finish();
}
criterion_group!(benches, packet);
criterion_main!(benches);
