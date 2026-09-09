use opennow_streamer_platform_windows::{WindowsBackend, WindowsGraphicsApi};

fn main() {
    for api in [WindowsGraphicsApi::D3d11, WindowsGraphicsApi::D3d12] {
        println!("{api:?}: {:#?}", WindowsBackend::probe_for(api));
    }
}
