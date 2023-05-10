use anyhow::Result;
use clap::ValueEnum;
use heck::ToKebabCase;
use necessist_core::{
    framework::{
        Applicable, AsParse, AsRun, Interface, Parse as ParseHigh, Postprocess, Run as RunHigh,
        ToImplementation,
    },
    LightContext, Span,
};
use std::{cell::RefCell, path::Path, rc::Rc};
use strum_macros::EnumIter;
use subprocess::Exec;

// Framework modules

/* mod foundry;
use foundry::Foundry;

mod golang;
use golang::Golang; */

mod hardhat_ts;
use hardhat_ts::HardhatTs;

mod rust;
use rust::Rust;

// Other modules

mod parsing;
use parsing::{AbstractTypes, MaybeNamed, Named, ParseAdapter, ParseLow, Spanned, WalkDirResult};

mod generic_visitor;
use generic_visitor::GenericVisitor;

mod running;
use running::{ProcessLines, RunAdapter, RunLow};

mod ts_utils;

#[derive(Debug, Clone, Copy, EnumIter, Eq, Ord, PartialEq, PartialOrd, ValueEnum)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[non_exhaustive]
#[remain::sorted]
pub enum Identifier {
    /* Foundry,
    Golang, */
    HardhatTs,
    Rust,
}

impl Applicable for Identifier {
    fn applicable(&self, context: &LightContext) -> Result<bool> {
        match *self {
            /* Self::Foundry => Foundry::applicable(context),
            Self::Golang => Golang::applicable(context), */
            Self::HardhatTs => HardhatTs::applicable(context),
            Self::Rust => Rust::applicable(context),
        }
    }
}

impl ToImplementation for Identifier {
    fn to_implementation(&self, _context: &LightContext) -> Result<Option<Box<dyn Interface>>> {
        Ok(Some(match *self {
            /* Self::Foundry => implementation_as_interface(ParseRunAdapter::new)(Foundry::new()),

            Self::Golang => implementation_as_interface(ParseRunAdapter::new)(Golang::new()),
            */
            // smoelius: `HardhatTs` implements the high-level `Run` interface directly.
            Self::HardhatTs => implementation_as_interface(ParseAdapter)(HardhatTs::new()),

            Self::Rust => implementation_as_interface(ParseRunAdapter::new)(Rust::new()),
        }))
    }
}

impl std::fmt::Display for Identifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", format!("{self:?}").to_kebab_case())
    }
}

/// Utility function
fn implementation_as_interface<T, U: Interface + 'static>(
    adapter: impl Fn(T) -> U,
) -> impl Fn(T) -> Box<dyn Interface> {
    move |implementation| Box::new(adapter(implementation)) as Box<dyn Interface>
}

impl<T: RunHigh> RunHigh for ParseAdapter<T> {
    fn dry_run(&self, context: &LightContext, test_file: &Path) -> Result<()> {
        self.0.dry_run(context, test_file)
    }
    fn exec(
        &self,
        context: &LightContext,
        span: &Span,
    ) -> Result<Option<(Exec, Option<Box<Postprocess>>)>> {
        self.0.exec(context, span)
    }
}

impl<T: ParseLow + RunHigh> Interface for ParseAdapter<T> {}

struct ParseRunAdapter<T> {
    parse: ParseAdapter<Rc<RefCell<T>>>,
    run: RunAdapter<Rc<RefCell<T>>>,
}

impl<T> ParseRunAdapter<T> {
    fn new(value: T) -> Self {
        let rc = Rc::new(RefCell::new(value));
        Self {
            parse: ParseAdapter(rc.clone()),
            run: RunAdapter(rc),
        }
    }
}

impl<T: ParseLow> AsParse for ParseRunAdapter<T> {
    fn as_parse(&self) -> &dyn ParseHigh {
        &self.parse
    }
    fn as_parse_mut(&mut self) -> &mut dyn ParseHigh {
        &mut self.parse
    }
}

impl<T: RunLow> AsRun for ParseRunAdapter<T> {
    fn as_run(&self) -> &dyn RunHigh {
        &self.run
    }
}

impl<T: ParseLow + RunLow> Interface for ParseRunAdapter<T> {}
