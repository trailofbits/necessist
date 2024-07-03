mod solang;
mod syn;

fn main() {
    solang::emit();
    syn::emit();

    println!("cargo:rerun-if-changed=assets");
}
