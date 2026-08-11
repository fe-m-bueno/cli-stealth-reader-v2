//! The slash-command catalogue.
//!
//! Definitions are data, not behavior: parsing, help, and completion all read
//! from this one list, so a new command needs no changes elsewhere. Order is the
//! order the manual and the `COMMANDS` section list them.

/// A positional argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArgSpec {
    pub name: &'static str,
    pub required: bool,
}

const fn arg(name: &'static str) -> ArgSpec {
    ArgSpec {
        name,
        required: false,
    }
}

const fn required_arg(name: &'static str) -> ArgSpec {
    ArgSpec {
        name,
        required: true,
    }
}

/// A `--flag`, optionally with a single-letter alias and a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlagSpec {
    pub name: &'static str,
    pub alias: Option<char>,
    pub takes_value: bool,
}

const fn flag(name: &'static str) -> FlagSpec {
    FlagSpec {
        name,
        alias: None,
        takes_value: false,
    }
}

const fn aliased_flag(name: &'static str, alias: char) -> FlagSpec {
    FlagSpec {
        name,
        alias: Some(alias),
        takes_value: false,
    }
}

const fn value_flag(name: &'static str) -> FlagSpec {
    FlagSpec {
        name,
        alias: None,
        takes_value: true,
    }
}

/// Grouping shown next to a suggestion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Navigation,
    Library,
    Annotations,
    Data,
    Appearance,
    Settings,
    Integrations,
    Help,
}

impl Category {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Navigation => "Navigation",
            Self::Library => "Library",
            Self::Annotations => "Annotations",
            Self::Data => "Data",
            Self::Appearance => "Appearance",
            Self::Settings => "Settings",
            Self::Integrations => "Integrations",
            Self::Help => "Help",
        }
    }
}

/// One command's complete contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandSpec {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub category: Category,
    pub description: &'static str,
    pub args: &'static [ArgSpec],
    pub flags: &'static [FlagSpec],
    pub usage: &'static str,
    pub details: &'static [&'static str],
    pub examples: &'static [&'static str],
    pub notes: &'static [&'static str],
}

impl CommandSpec {
    /// Whether `name` is this command's name or one of its aliases.
    #[must_use]
    pub fn answers_to(&self, name: &str) -> bool {
        self.name == name || self.aliases.contains(&name)
    }

    #[must_use]
    pub fn flag(&self, name: &str) -> Option<&FlagSpec> {
        self.flags.iter().find(|spec| spec.name == name)
    }

    #[must_use]
    pub fn flag_by_alias(&self, alias: char) -> Option<&FlagSpec> {
        self.flags.iter().find(|spec| spec.alias == Some(alias))
    }
}

const NO_ARGS: &[ArgSpec] = &[];
const NO_FLAGS: &[FlagSpec] = &[];
const NO_ALIASES: &[&str] = &[];
const NO_NOTES: &[&str] = &[];

/// Every command, in manual order.
pub const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "prev",
        aliases: NO_ALIASES,
        category: Category::Navigation,
        description: "Go to previous chapter.",
        args: &[arg("count")],
        flags: NO_FLAGS,
        usage: "/prev [count]",
        details: &["Moves backward by one chapter, or by count chapters when count is provided."],
        examples: &["/prev", "/prev 3"],
        notes: NO_NOTES,
    },
    CommandSpec {
        name: "next",
        aliases: NO_ALIASES,
        category: Category::Navigation,
        description: "Go to next chapter.",
        args: &[arg("count")],
        flags: NO_FLAGS,
        usage: "/next [count]",
        details: &["Moves forward by one chapter, or by count chapters when count is provided."],
        examples: &["/next", "/next 2"],
        notes: NO_NOTES,
    },
    CommandSpec {
        name: "chapters",
        aliases: NO_ALIASES,
        category: Category::Navigation,
        description: "Open the table of contents.",
        args: &[arg("query")],
        flags: &[flag("current"), flag("flat")],
        usage: "/chapters [query] [--current] [--flat]",
        details: &[
            "Opens the chapter picker. With a query, the list is filtered to matching chapter titles.",
            "--current starts the picker at the current chapter. --flat shows a flattened table of contents.",
        ],
        examples: &[
            "/chapters",
            "/chapters introduction",
            "/chapters --current",
            "/chapters appendix --flat",
        ],
        notes: NO_NOTES,
    },
    CommandSpec {
        name: "changebook",
        aliases: &["book"],
        category: Category::Library,
        description: "Switch books from the library or current folder.",
        args: &[arg("query")],
        flags: &[flag("recent"), flag("cwd"), value_flag("sort")],
        usage: "/changebook [query] [--recent] [--cwd] [--sort lastOpened|title|author|progress]",
        details: &[
            "Opens the book library and optionally filters it by title, author, or file name.",
            "--recent prioritizes recently opened books. --cwd shows books discovered in the current folder.",
            "--sort changes the library sort key for this picker.",
        ],
        examples: &[
            "/changebook",
            "/book dune",
            "/changebook --recent",
            "/changebook --sort progress",
        ],
        notes: NO_NOTES,
    },
    CommandSpec {
        name: "colorscheme",
        aliases: NO_ALIASES,
        category: Category::Appearance,
        description: "Change the active color scheme.",
        args: &[arg("scheme")],
        flags: &[flag("preview"), flag("list")],
        usage: "/colorscheme [scheme] [--preview] [--list]",
        details: &[
            "Without a scheme, opens the colorscheme picker. With a scheme id, applies that colorscheme.",
            "Color schemes control the accent hue family and remain separate from light/dark theming.",
            "--preview is accepted for compatibility. --list opens the full colorscheme list.",
        ],
        examples: &[
            "/colorscheme",
            "/colorscheme claude",
            "/colorscheme forest --preview",
            "/colorscheme --list",
        ],
        notes: NO_NOTES,
    },
    CommandSpec {
        name: "theme",
        aliases: NO_ALIASES,
        category: Category::Appearance,
        description: "Change the active appearance theme.",
        args: &[arg("theme")],
        flags: &[flag("list")],
        usage: "/theme [theme] [--list]",
        details: &[
            "Without a theme, opens the appearance theme picker. With a theme id, applies that appearance.",
            "Themes control dark/light, colorblind-friendly, and ANSI-only rendering without changing the active colorscheme.",
        ],
        examples: &[
            "/theme",
            "/theme light",
            "/theme dark-colorblind",
            "/theme light-ansi",
            "/theme --list",
        ],
        notes: NO_NOTES,
    },
    CommandSpec {
        name: "resume",
        aliases: NO_ALIASES,
        category: Category::Library,
        description: "Resume the latest or a specific book.",
        args: &[arg("book-query")],
        flags: &[flag("latest")],
        usage: "/resume [book-query] [--latest]",
        details: &["Reopens a book from the library at its saved reading position."],
        examples: &["/resume", "/resume --latest", "/resume hobbit"],
        notes: NO_NOTES,
    },
    CommandSpec {
        name: "add",
        aliases: NO_ALIASES,
        category: Category::Library,
        description: "Import an EPUB, CBZ, or PDF from the library directory or an explicit path.",
        args: &[arg("path")],
        flags: &[flag("cwd"), flag("force")],
        usage: "/add [path] [--cwd] [--force]",
        details: &[
            "Imports a supported book file into the local library and opens it.",
            "Without a path, opens a recursive file picker rooted at the configured library directory.",
            "--cwd temporarily uses the current working directory instead.",
            "--force reimports even when the file hash already exists in the library.",
        ],
        examples: &[
            "/add",
            "/add ./books/example.epub",
            "/add --cwd",
            "/add ./comic.cbz --force",
        ],
        notes: NO_NOTES,
    },
    CommandSpec {
        name: "librarydir",
        aliases: &["bookdir"],
        category: Category::Library,
        description: "Show or configure the directory scanned for books.",
        args: &[arg("path")],
        flags: &[flag("cwd")],
        usage: "/librarydir [path] [--cwd]",
        details: &[
            "Stores an absolute library directory and scans it recursively for EPUB, CBZ, and PDF files.",
            "With no path, shows the active directory. --cwd resets discovery to the process working directory.",
        ],
        examples: &["/librarydir", "/librarydir ~/Books", "/librarydir --cwd"],
        notes: NO_NOTES,
    },
    CommandSpec {
        name: "remove",
        aliases: NO_ALIASES,
        category: Category::Library,
        description: "Remove a book from the library.",
        args: &[arg("book-query")],
        flags: &[flag("current")],
        usage: "/remove [book-query] [--current]",
        details: &[
            "Removes a matching book from the library database.",
            "--current removes the book currently open in the reader.",
        ],
        examples: &["/remove dune", "/remove --current"],
        notes: NO_NOTES,
    },
    CommandSpec {
        name: "removecurrent",
        aliases: NO_ALIASES,
        category: Category::Library,
        description: "Remove the current book from the library.",
        args: NO_ARGS,
        flags: &[flag("confirm")],
        usage: "/removecurrent [--confirm]",
        details: &[
            "Removes the active book. The command requires --confirm to avoid accidental deletion.",
        ],
        examples: &["/removecurrent --confirm"],
        notes: NO_NOTES,
    },
    CommandSpec {
        name: "toggleprogress",
        aliases: NO_ALIASES,
        category: Category::Appearance,
        description: "Set progress display mode.",
        args: &[arg("mode")],
        flags: NO_FLAGS,
        usage: "/toggleprogress [time-chapter|time-book|book|both|chapter|hidden]",
        details: &[
            "With no argument, cycles through progress display modes.",
            "time-chapter and time-book show estimated remaining reading time from learned pace.",
            "book/chapter/both show percentage bars; hidden disables the footer progress line.",
        ],
        examples: &[
            "/toggleprogress",
            "/toggleprogress time-chapter",
            "/toggleprogress both",
            "/toggleprogress hidden",
        ],
        notes: NO_NOTES,
    },
    CommandSpec {
        name: "mode",
        aliases: NO_ALIASES,
        category: Category::Appearance,
        description: "Switch rendering mode or code language.",
        args: &[required_arg("mode")],
        flags: NO_FLAGS,
        usage: "/mode [plain|typescript|python|rust]",
        details: &[
            "plain renders the book as prose. typescript, python, and rust render the text through the selected code-like stealth style.",
            "The selected mode is saved and reused the next time the app starts.",
        ],
        examples: &[
            "/mode plain",
            "/mode typescript",
            "/mode python",
            "/mode rust",
        ],
        notes: NO_NOTES,
    },
    CommandSpec {
        name: "highlight",
        aliases: NO_ALIASES,
        category: Category::Appearance,
        description: "Toggle plain-mode dialogue highlight.",
        args: &[required_arg("state")],
        flags: NO_FLAGS,
        usage: "/highlight <on|off>",
        details: &["Enables or disables dialogue highlighting while using plain render mode."],
        examples: &["/highlight on", "/highlight off"],
        notes: NO_NOTES,
    },
    CommandSpec {
        name: "mouse",
        aliases: NO_ALIASES,
        category: Category::Settings,
        description: "Toggle mouse capture for the draggable scrollbar.",
        args: &[arg("state")],
        flags: NO_FLAGS,
        usage: "/mouse [on|off]",
        details: &[
            "off keeps native terminal text selection and wheel scrolling. on enables app mouse capture so the in-app scrollbar can be clicked and dragged.",
            "Terminal protocols cannot provide native drag selection and app scrollbar dragging from the same unmodified mouse gesture.",
        ],
        examples: &["/mouse on", "/mouse off"],
        notes: &[
            "When mouse capture is on, most terminals still allow text selection with Shift-drag.",
        ],
    },
    CommandSpec {
        name: "settings",
        aliases: &["config"],
        category: Category::Settings,
        description: "Open tabbed reader settings with a live preview.",
        args: NO_ARGS,
        flags: NO_FLAGS,
        usage: "/settings",
        details: &[
            "Opens Kindle-style Themes, Reading, Layout, and More tabs.",
            "Use Left/Right to change tabs, Up/Down to select, Space to change, Enter to save, / to search, and Esc to cancel.",
        ],
        examples: &["/settings", "/config"],
        notes: &["Shortcut: press S from the reader to open settings."],
    },
    CommandSpec {
        name: "toggl",
        aliases: NO_ALIASES,
        category: Category::Integrations,
        description: "Log reading time directly to Toggl 2.0.",
        args: &[arg("action"), arg("description")],
        flags: &[
            value_flag("project"),
            value_flag("duration"),
            flag("open"),
            flag("disconnect"),
        ],
        usage: "/toggl auth|setup|sync|recent|start|stop|log [description] [--project name] [--duration 25m]",
        details: &[
            "auth opens the Toggl 2.0 key page; connect with /toggl auth <toggl_sk_...>.",
            "If Focus cannot return the organization ID, setup asks once for the workspace URL and stores the extracted ID.",
            "sync caches recent projects and descriptions from Focus for fuzzy project lookup.",
            "start creates a running timer, stop stops the current timer, and log creates a finished entry ending now.",
        ],
        examples: &[
            "/toggl auth",
            "/toggl auth <toggl_sk_...>",
            "/toggl sync",
            "/toggl start \"O Nome do Vento\" --project \"Reading books\"",
            "/toggl log \"Choujin X\" --project \"Reading manga\" --duration 45m",
            "/toggl stop",
        ],
        notes: &[
            "Uses the Toggl 2.0 Focus API with Bearer authentication. The key is stored in the local app settings database.",
            "Quota is per user and organization: Free 30/h, Starter 240/h, Premium 600/h. Remaining requests and reset time come from response headers.",
        ],
    },
    CommandSpec {
        name: "help",
        aliases: NO_ALIASES,
        category: Category::Help,
        description: "Show help for commands.",
        args: &[arg("command")],
        flags: &[flag("all")],
        usage: "/help [command] [--all]",
        details: &[
            "Opens a full-page manual. With a command name or alias, opens the manual entry for that command.",
            "--all is accepted for compatibility and shows the complete command manual.",
        ],
        examples: &["/help", "/help mode", "/help theme", "/help --all"],
        notes: &[
            "Use Ctrl+. (or Ctrl+X as a terminal fallback) or /keyboardshortcuts for keyboard shortcut help.",
        ],
    },
    CommandSpec {
        name: "keyboardshortcuts",
        aliases: &["keys"],
        category: Category::Help,
        description: "Show keyboard shortcuts.",
        args: NO_ARGS,
        flags: &[value_flag("category")],
        usage: "/keyboardshortcuts [--category navigation|commands|view]",
        details: &[
            "Shows keyboard shortcuts. Use --category to focus navigation, command, or view shortcuts.",
        ],
        examples: &[
            "/keyboardshortcuts",
            "/keys",
            "/keys --category navigation",
            "/keyboardshortcuts --category commands",
        ],
        notes: NO_NOTES,
    },
    CommandSpec {
        name: "density",
        aliases: NO_ALIASES,
        category: Category::Appearance,
        description: "Set code density (1=max comments, 5=max code). The d key cycles through 1→3→5.",
        args: &[arg("level")],
        flags: NO_FLAGS,
        usage: "/density [1-5]",
        details: &[
            "Controls how dense the code-style renderers are. Lower values favor explanatory comment-like text; higher values favor compact code-like output.",
        ],
        examples: &["/density 1", "/density 3", "/density 5"],
        notes: &["In code mode, the d key cycles through 1, 3, and 5."],
    },
    CommandSpec {
        name: "goto",
        aliases: NO_ALIASES,
        category: Category::Navigation,
        description: "Jump by book %, chapter %, or chapter number.",
        args: &[required_arg("position")],
        flags: &[flag("chapter")],
        usage: "/goto <n|%> [--chapter]",
        details: &[
            "Jumps to a chapter number or a percentage position.",
            "A bare number is treated as a chapter number. A value ending in % is treated as whole-book progress.",
            "Use --chapter, or the shorthand %c form, to treat a percentage as progress within the current chapter.",
        ],
        examples: &["/goto 5", "/goto 42%", "/goto 30% --chapter", "/goto 30%c"],
        notes: NO_NOTES,
    },
    CommandSpec {
        name: "search",
        aliases: NO_ALIASES,
        category: Category::Navigation,
        description: "Search in the current chapter; use -g or --global for the whole book.",
        args: &[arg("term")],
        flags: &[aliased_flag("global", 'g')],
        usage: "/search [-g|--global] <term>",
        details: &[
            "Searches for text and highlights matches. By default the search is limited to the current chapter.",
            "-g or --global searches the entire book.",
        ],
        examples: &[
            "/search ring",
            "/search \"chapter one\"",
            "/search -g mordor",
            "/search --global \"needle in a haystack\"",
        ],
        notes: &["After a search, use n and N to move between matches."],
    },
    CommandSpec {
        name: "mark",
        aliases: NO_ALIASES,
        category: Category::Annotations,
        description: "Save a bookmark at the current reading position.",
        args: &[arg("label")],
        flags: NO_FLAGS,
        usage: "/mark [label]",
        details: &[
            "Creates a bookmark at the current chapter and block offset. The optional label makes it easier to find later.",
        ],
        examples: &["/mark", "/mark important reveal", "/mark \"return here\""],
        notes: NO_NOTES,
    },
    CommandSpec {
        name: "marks",
        aliases: NO_ALIASES,
        category: Category::Annotations,
        description: "Open bookmarks for the current book.",
        args: NO_ARGS,
        flags: NO_FLAGS,
        usage: "/marks",
        details: &["Opens the bookmark picker for the current book."],
        examples: &["/marks"],
        notes: &[
            "Inside the bookmark picker, press Enter to jump to a bookmark or d to delete the selected bookmark.",
        ],
    },
    CommandSpec {
        name: "delmark",
        aliases: NO_ALIASES,
        category: Category::Annotations,
        description: "Delete a bookmark by id or label.",
        args: &[required_arg("id-or-label")],
        flags: NO_FLAGS,
        usage: "/delmark <id|label>",
        details: &["Deletes the bookmark whose id or label matches the argument."],
        examples: &["/delmark 01HR...", "/delmark \"return here\""],
        notes: NO_NOTES,
    },
    CommandSpec {
        name: "export",
        aliases: NO_ALIASES,
        category: Category::Data,
        description: "Export reading state (positions, bookmarks, notes, tags) to JSON.",
        args: &[arg("path")],
        flags: NO_FLAGS,
        usage: "/export [path]",
        details: &[
            "Writes reading positions, bookmarks, notes, and tags to a JSON file.",
            "When no path is supplied, the app chooses a default export path.",
        ],
        examples: &["/export", "/export ./reader-backup.json"],
        notes: NO_NOTES,
    },
    CommandSpec {
        name: "import",
        aliases: NO_ALIASES,
        category: Category::Data,
        description: "Import and merge reading state from a JSON export file.",
        args: &[arg("path")],
        flags: NO_FLAGS,
        usage: "/import [path]",
        details: &[
            "Reads a JSON export and merges matching positions, bookmarks, notes, and tags into the local library.",
        ],
        examples: &["/import ./reader-backup.json"],
        notes: NO_NOTES,
    },
    CommandSpec {
        name: "tag",
        aliases: NO_ALIASES,
        category: Category::Annotations,
        description: "Add, remove, or list tags for the current book.",
        args: &[arg("tag")],
        flags: &[aliased_flag("delete", 'd')],
        usage: "/tag [tag] [-d <tag>]",
        details: &[
            "With no argument, lists tags for the current book. With a tag argument, adds that tag.",
            "-d or --delete removes a tag from the current book.",
        ],
        examples: &["/tag", "/tag favorite", "/tag sci-fi", "/tag -d favorite"],
        notes: NO_NOTES,
    },
    CommandSpec {
        name: "tags",
        aliases: NO_ALIASES,
        category: Category::Annotations,
        description: "List tags for the current book.",
        args: NO_ARGS,
        flags: NO_FLAGS,
        usage: "/tags",
        details: &["Lists all tags assigned to the current book."],
        examples: &["/tags"],
        notes: NO_NOTES,
    },
    CommandSpec {
        name: "note",
        aliases: NO_ALIASES,
        category: Category::Annotations,
        description: "Add a note at current position, list notes (-l), or delete a note (-d <id>).",
        args: &[arg("text")],
        flags: &[aliased_flag("list", 'l'), aliased_flag("delete", 'd')],
        usage: "/note [text] [-l] [-d <id>]",
        details: &[
            "With text, creates a note at the current reading position.",
            "-l or --list opens the notes list. -d or --delete deletes a note by id.",
        ],
        examples: &[
            "/note remember this",
            "/note \"check this quote later\"",
            "/note -l",
            "/note -d 01HR...",
        ],
        notes: &[
            "Inside the notes list, press Enter to jump to a note or d to delete the selected note.",
        ],
    },
];

/// The command answering to `name`, which may be an alias.
#[must_use]
pub fn find_command(name: &str) -> Option<&'static CommandSpec> {
    COMMANDS.iter().find(|command| command.answers_to(name))
}

#[cfg(test)]
mod tests {
    use super::{COMMANDS, find_command};

    #[test]
    fn every_command_is_reachable_by_name_and_alias() {
        for command in COMMANDS {
            assert_eq!(
                find_command(command.name).map(|found| found.name),
                Some(command.name)
            );
            for alias in command.aliases {
                assert_eq!(
                    find_command(alias).map(|found| found.name),
                    Some(command.name),
                    "alias /{alias} did not resolve"
                );
            }
        }
        assert!(find_command("nope").is_none());
    }

    #[test]
    fn names_and_aliases_are_unique() {
        let mut seen: Vec<&str> = Vec::new();
        for command in COMMANDS {
            for name in std::iter::once(&command.name).chain(command.aliases.iter()) {
                assert!(!seen.contains(name), "/{name} is defined twice");
                seen.push(name);
            }
        }
    }

    #[test]
    fn every_command_documents_itself() {
        for command in COMMANDS {
            assert!(
                command.usage.starts_with(&format!("/{}", command.name)),
                "/{} has a usage line that does not start with its name",
                command.name
            );
            assert!(!command.description.is_empty());
            assert!(
                !command.examples.is_empty(),
                "/{} has no example",
                command.name
            );
            for argument in command.args {
                assert!(!argument.name.is_empty());
            }
        }
    }

    #[test]
    fn value_flags_have_no_short_alias() {
        // A short alias cannot carry a value, so the parser rejects the
        // combination; the catalogue must not create one.
        for command in COMMANDS {
            for spec in command.flags {
                assert!(
                    !(spec.takes_value && spec.alias.is_some()),
                    "/{} --{} has both a value and an alias",
                    command.name,
                    spec.name
                );
            }
        }
    }
}
