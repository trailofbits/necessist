use proc_macro2::{TokenStream, TokenTree};
use std::{
    env::var,
    fs::{File, OpenOptions, read_to_string},
    io::{Error, Write},
    path::Path,
};
use syn::{File as SynFile, Item, ItemMacro, ItemStruct, Type, TypePath, parse_file, parse2};

pub fn emit() {
    let out_dir = var("OUT_DIR").unwrap();

    let contents = read_to_string("assets/syn_expr.rs").unwrap();
    let syn_file =
        parse_file(&contents).unwrap_or_else(|_| panic!("Failed to parse: {contents:?}"));

    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(Path::new(&out_dir).join("expression_with_block.rs"))
        .unwrap();

    emit_expression_with_block(&mut file, &syn_file).unwrap();
}

fn emit_expression_with_block(file: &mut File, syn_file: &SynFile) -> Result<(), Error> {
    let mut variants = Vec::new();

    for item in &syn_file.items {
        let Item::Macro(ItemMacro { mac, .. }) = item else {
            continue;
        };
        if !mac.path.is_ident("ast_struct") {
            continue;
        }
        let Ok(item_struct) = parse2::<ItemStruct>(filter_tokens(mac.tokens.clone())) else {
            continue;
        };
        if !item_struct.fields.iter().any(|field| {
            if let Type::Path(TypePath { qself: None, path }) = &field.ty {
                path.is_ident("Block")
            } else {
                false
            }
        }) {
            continue;
        }
        let Some(variant) = item_struct
            .ident
            .to_string()
            .strip_prefix("Expr")
            .map(ToOwned::to_owned)
        else {
            continue;
        };
        variants.push(variant);
    }

    write!(
        file,
        "\
fn expression_with_block(expr: &syn::Expr) -> bool {{
    matches!(expr,"
    )?;
    let mut first = true;
    for variant in variants {
        if !first {
            write!(file, " |").unwrap();
        }
        first = false;
        write!(file, " syn::Expr::{variant}(_)").unwrap();
    }
    writeln!(
        file,
        ")
}}"
    )?;

    Ok(())
}

fn filter_tokens(token_stream: TokenStream) -> TokenStream {
    let mut tokens = Vec::new();
    let mut iter = token_stream.into_iter().peekable();
    while let Some(token) = iter.next() {
        if token.to_string() == "#"
            && iter.peek().map(ToString::to_string) == Some(String::from("full"))
        {
            let _: Option<TokenTree> = iter.next();
            continue;
        }
        tokens.push(token);
    }
    tokens.into_iter().collect()
}
