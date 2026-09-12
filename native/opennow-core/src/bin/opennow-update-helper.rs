use opennow_core::update_apply;

fn main() {
    let mut arguments = std::env::args_os().skip(1);
    let result = match (arguments.next(), arguments.next(), arguments.next()) {
        (Some(flag), Some(path), None) if flag == "--apply" => {
            update_apply::run_helper(std::path::Path::new(&path))
        }
        _ => Err("Usage: opennow-update-helper --apply <prepared-plan.json>".to_owned()),
    };
    if let Err(error) = result {
        eprintln!("opennow-update-helper: {error}");
        std::process::exit(1);
    }
}
