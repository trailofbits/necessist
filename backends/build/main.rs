mod solang;
mod syn;

fn main() {
    println!("cargo::rustc-check-cfg=cfg(sort_walk_dir_results)");

    if std::env::var_os("CI").is_some() {
        println!("cargo::rustc-cfg=sort_walk_dir_results");
    }

    solang::emit();
    syn::emit();

    println!("cargo::rerun-if-env-changed=CI");
    println!("cargo::rerun-if-changed=assets");
}
