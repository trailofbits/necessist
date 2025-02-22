use std::{
    env::var,
    fs::{File, OpenOptions, read_to_string},
    io::{Error, Write},
    path::Path,
};
use syn::{Fields, File as SynFile, Ident, Item, ItemEnum, ItemStruct, Variant, parse_file};

pub fn emit() {
    let out_dir = var("OUT_DIR").unwrap();

    let contents = read_to_string("assets/solang_parser_pt.rs").unwrap();
    let syn_file =
        parse_file(&contents).unwrap_or_else(|_| panic!("Failed to parse: {contents:?}"));

    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(Path::new(&out_dir).join("visit.rs"))
        .unwrap();

    emit_visitable_impls(&mut file, &syn_file).unwrap();
    emit_visitor_trait(&mut file, &syn_file).unwrap();
    emit_visit_fns(&mut file, &syn_file).unwrap();
}

fn emit_visitable_impls(file: &mut File, syn_file: &SynFile) -> Result<(), Error> {
    for item in &syn_file.items {
        match item {
            Item::Enum(ItemEnum { ident, .. }) | Item::Struct(ItemStruct { ident, .. }) => {
                writeln!(
                    file,
                    "
impl Visitable for {} {{
    fn visit<'ast, V>(&'ast self, v: &mut V) -> Result<(), V::Error>
    where
        V: Visitor<'ast> + ?Sized
    {{
        v.visit_{}(self)
    }}
}}",
                    ident,
                    unreserved_name(ident)
                )?;
            }
            _ => {}
        }
    }

    Ok(())
}

fn emit_visitor_trait(file: &mut File, syn_file: &SynFile) -> Result<(), Error> {
    writeln!(
        file,
        "
pub trait Visitor<'ast> {{
    type Error;"
    )?;
    for item in &syn_file.items {
        match item {
            Item::Enum(ItemEnum { ident, .. }) | Item::Struct(ItemStruct { ident, .. }) => {
                writeln!(
                    file,
                    "
    fn visit_{}(&mut self, {0}: &'ast {}) -> Result<(), Self::Error> {{
        visit_{0}(self, {0})
    }}",
                    unreserved_name(ident),
                    ident
                )?;
            }
            _ => {}
        }
    }
    writeln!(file, "}}")?;

    Ok(())
}

fn emit_visit_fns(file: &mut File, syn_file: &SynFile) -> Result<(), Error> {
    for item in &syn_file.items {
        match item {
            Item::Enum(item_enum) => {
                emit_visit_enum_fn(file, syn_file, item_enum)?;
            }
            Item::Struct(item_struct) => {
                emit_visit_struct_fn(file, syn_file, item_struct)?;
            }
            _ => {}
        }
    }

    Ok(())
}

fn emit_visit_enum_fn(
    file: &mut File,
    _syn_file: &SynFile,
    item_enum: &ItemEnum,
) -> Result<(), Error> {
    let ItemEnum {
        ident: enum_ident,
        variants,
        ..
    } = item_enum;

    writeln!(file)?;

    // smoelius: Hack. Currently, `allow(unused_variables)` is needed only for `visit_function_ty`.
    if variants.iter().all(|variant| {
        matches!(
            variant,
            Variant {
                fields: Fields::Unit,
                ..
            }
        )
    }) {
        writeln!(file, "#[allow(unused_variables)]")?;
    }

    writeln!(
        file,
        "pub fn visit_{}<'ast, V>(visitor: &mut V, {0}: &'ast {}) -> Result<(), V::Error>
where
    V: Visitor<'ast> + ?Sized
{{
    match {0} {{",
        unreserved_name(enum_ident),
        enum_ident
    )?;

    for Variant {
        ident: variant_ident,
        fields,
        ..
    } in variants
    {
        write!(file, "        {enum_ident}::{variant_ident}")?;
        match fields {
            Fields::Named(fields) => {
                write!(file, "{{ ")?;
                for (i, field) in fields.named.iter().enumerate() {
                    if i != 0 {
                        write!(file, ", ")?;
                    }
                    write!(file, "{}", field.ident.as_ref().unwrap())?;
                }
                write!(file, " }}")?;
            }
            Fields::Unnamed(fields) => {
                write!(file, "(")?;
                for (i, _) in fields.unnamed.iter().enumerate() {
                    if i != 0 {
                        write!(file, ", ")?;
                    }
                    write!(file, "unnamed_{i}")?;
                }
                write!(file, ")")?;
            }
            Fields::Unit => {}
        }
        writeln!(file, " => {{")?;
        match fields {
            Fields::Named(fields) => {
                for field in &fields.named {
                    writeln!(
                        file,
                        "            {}.visit(visitor)?;",
                        field.ident.as_ref().unwrap()
                    )?;
                }
            }
            Fields::Unnamed(fields) => {
                for (i, _) in fields.unnamed.iter().enumerate() {
                    writeln!(file, "            unnamed_{i}.visit(visitor)?;")?;
                }
            }
            Fields::Unit => {}
        }
        writeln!(
            file,
            "            Ok(())
        }}"
        )?;
    }

    writeln!(
        file,
        "    }}
}}"
    )?;

    Ok(())
}

fn emit_visit_struct_fn(
    file: &mut File,
    _syn_file: &SynFile,
    item_struct: &ItemStruct,
) -> Result<(), Error> {
    let ItemStruct {
        ident: struct_ident,
        fields,
        ..
    } = item_struct;

    writeln!(
        file,
        "
pub fn visit_{}<'ast, V>(visitor: &mut V, {0}: &'ast {}) -> Result<(), V::Error>
where
    V: Visitor<'ast> + ?Sized
{{",
        unreserved_name(struct_ident),
        struct_ident
    )?;

    match fields {
        Fields::Named(fields) => {
            for field in &fields.named {
                writeln!(
                    file,
                    "    {}.{}.visit(visitor)?;",
                    unreserved_name(struct_ident),
                    field.ident.as_ref().unwrap()
                )?;
            }
        }
        Fields::Unnamed(fields) => {
            for (i, _) in fields.unnamed.iter().enumerate() {
                writeln!(
                    file,
                    "    {}.{}.visit(visitor)?;",
                    unreserved_name(struct_ident),
                    i
                )?;
            }
        }
        Fields::Unit => {}
    }

    writeln!(
        file,
        "    Ok(())
}}"
    )?;

    Ok(())
}

fn unreserved_name(ident: &Ident) -> String {
    let candidate = <_ as heck::ToSnakeCase>::to_snake_case(ident.to_string().as_str());
    match candidate.as_str() {
        "type" => "ty".to_owned(),
        _ => candidate,
    }
}
