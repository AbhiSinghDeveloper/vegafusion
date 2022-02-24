use criterion::async_executor::AsyncExecutor;
use std::fs;
use vegafusion_core::planning::plan::SpecPlan;
use vegafusion_core::planning::watch::{ExportUpdateBatch, WatchPlan};
use vegafusion_core::proto::gen::services::query_request::Request;
use vegafusion_core::proto::gen::services::QueryRequest;
use vegafusion_core::proto::gen::tasks::{TaskGraph, TaskGraphValueRequest};
use vegafusion_core::spec::chart::ChartSpec;
use vegafusion_core::task_graph::task_value::TaskValue;
use vegafusion_rt_datafusion::task_graph::runtime::TaskGraphRuntime;

fn crate_dir() -> String {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .display()
        .to_string()
}

fn load_spec(spec_name: &str) -> ChartSpec {
    // Load spec
    let spec_path = format!("{}/benches/specs/{}.vg.json", crate_dir(), spec_name);
    let spec_str = fs::read_to_string(spec_path).unwrap();
    serde_json::from_str(&spec_str).unwrap()
}

fn load_updates(spec_name: &str) -> Vec<ExportUpdateBatch> {
    let updates_path = format!("{}/benches/specs/{}.updates.json", crate_dir(), spec_name);
    let updates_path = std::path::Path::new(&updates_path);

    if updates_path.exists() {
        let updates_str = fs::read_to_string(updates_path).unwrap();
        serde_json::from_str(&updates_str).unwrap()
    } else {
        Vec::new()
    }
}

async fn eval_spec_sequence_from_files(spec_name: &str) {
    // Load spec
    let full_spec = load_spec(spec_name);

    // Load updates
    let full_updates = load_updates(spec_name);
    eval_spec_sequence(full_spec, full_updates).await
}

async fn eval_spec_sequence(full_spec: ChartSpec, full_updates: Vec<ExportUpdateBatch>) {
    let spec_plan = SpecPlan::try_new(&full_spec).unwrap();
    let task_scope = spec_plan.server_spec.to_task_scope().unwrap();

    // println!(
    //     "client_spec: {}",
    //     serde_json::to_string_pretty(&spec_plan.client_spec).unwrap()
    // );
    // println!(
    //     "server_spec: {}",
    //     serde_json::to_string_pretty(&spec_plan.server_spec).unwrap()
    // );
    //
    // println!(
    //     "comm_plan:\n---\n{}\n---",
    //     serde_json::to_string_pretty(&WatchPlan::from(spec_plan.comm_plan.clone())).unwrap()
    // );

    // Build task graph
    let tasks = spec_plan.server_spec.to_tasks().unwrap();
    let mut task_graph = TaskGraph::new(tasks, &task_scope).unwrap();
    let task_graph_mapping = task_graph.build_mapping();

    // Initialize task graph runtime
    let runtime = TaskGraphRuntime::new(Some(64), None);

    for update_batch in full_updates {
        let mut query_indices = Vec::new();
        for update in update_batch {
            let var = update.to_scoped_var();
            let value = update.to_task_value();
            let node_index = task_graph_mapping.get(&var).unwrap().node_index;
            query_indices.extend(task_graph.update_value(node_index as usize, value).unwrap());
        }

        // Make Query reques
        let request = QueryRequest {
            request: Some(Request::TaskGraphValues(TaskGraphValueRequest {
                task_graph: Some(task_graph.clone()),
                indices: query_indices,
            })),
        };
        let response = runtime.query_request(request).await.unwrap();
    }
}

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use tokio::runtime::Runtime;

#[inline]
fn fibonacci(n: u64) -> u64 {
    match n {
        0 => 1,
        1 => 1,
        n => fibonacci(n - 1) + fibonacci(n - 2),
    }
}

pub fn flights_crossfilter(c: &mut Criterion) {
    // Load spec
    let spec_name = "flights_crossfilter";
    let full_spec = load_spec(spec_name);
    let full_updates = load_updates(spec_name);

    let tokio_runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    c.bench_function(spec_name, |b| {
        b.to_async(&tokio_runtime)
            .iter(|| eval_spec_sequence(full_spec.clone(), full_updates.clone()))
    });
}

pub fn flights_crossfilter_local_time(c: &mut Criterion) {
    // Initialize runtime
    let tokio_runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    // Load spec
    let spec_name = "flights_crossfilter_local_time";
    let full_spec = load_spec(spec_name);
    let full_updates = load_updates(spec_name);

    c.bench_function(spec_name, |b| {
        b.to_async(&tokio_runtime)
            .iter(|| eval_spec_sequence(full_spec.clone(), full_updates.clone()))
    });
}

criterion_group!(benches, flights_crossfilter, flights_crossfilter_local_time);
criterion_main!(benches);
