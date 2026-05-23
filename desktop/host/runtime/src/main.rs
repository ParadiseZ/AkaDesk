use host_runtime::service::RuntimeService;

fn main() {
    let service = RuntimeService::new();
    println!(
        "host-runtime skeleton initialized: {}",
        service.status().state_label
    );
}
