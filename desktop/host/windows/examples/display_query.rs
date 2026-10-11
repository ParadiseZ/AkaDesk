fn main() {
    #[cfg(windows)]
    {
        let backend = host_windows::display::WindowsDisplayBackend;
        for display in backend.list().expect("Display enumeration failed") {
            println!("Display: {:?}", display);
            match backend.details(&display.display_id.0) {
                Ok(details) => println!("Modes: {:?}\nScales: {:?}", details.modes, details.scales),
                Err(error) => println!("Control unavailable: {error}"),
            }
        }
    }
}
