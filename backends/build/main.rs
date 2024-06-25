mod solang;
mod syn;

fn main() {
    solang::emit();
    syn::emit();

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/solang_parser_pt.rs");
    println!("cargo:rerun-if-changed=assets/syn_expr.rs");
}
