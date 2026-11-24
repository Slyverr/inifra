pub(crate) mod ask;
pub(crate) mod cli;
pub(crate) mod configure;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum UiExit {
    Continue,
    Quit,
}

pub(crate) use ask::AskArgs;
pub(crate) use cli::AssistantCommand;
