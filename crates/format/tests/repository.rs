//! End to end: build a repository on disk, then check it.
//!
//! These guard the rules an author actually trips over, so each one names the
//! mistake rather than the invariant.

use std::path::{Path, PathBuf};

use stemin_format::{check, tree};

/// Write a file, making its directories.
#[expect(
    clippy::expect_used,
    reason = "a fixture that cannot be written is a broken test, and must stop it loudly"
)]
fn put(root: &Path, rel: &str, body: &str) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("makes the directory");
    }
    std::fs::write(path, body).expect("writes the file");
}

/// The deck the valid repository holds: a card, then a reference.
const FIELD: &str = "---\ntitle: What a field is\n---\n\n::: card\nA field.\n:::\n\n\
                     ::: reference ohms-law\n# Ohm's law\n\n::: equation\nI = V/R\n:::\n:::\n";

/// A small, valid repository: one domain, one section, one topic, one deck
/// with one reference, and one checkpoint in a folder.
fn valid(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("stemin-format-{name}"));
    let _ = std::fs::remove_dir_all(&root);
    put(&root, "index.md", "---\nformat: 2.0.0\norder: [phy]\n---\n");
    put(
        &root,
        "phy/index.md",
        "---\ntitle: Physics\nlang: en\norder: [eam]\n---\n",
    );
    put(
        &root,
        "phy/eam/index.md",
        "---\ntitle: Electromagnetism\norder: [fields]\n---\n",
    );
    put(
        &root,
        "phy/eam/fields/index.md",
        "---\ntitle: Fields\norder: [field]\n---\n",
    );
    put(&root, "phy/eam/fields/field.md", FIELD);
    put(
        &root,
        "phy/checkpoints/index.md",
        "---\norder: [mit]\n---\n",
    );
    put(
        &root,
        "phy/checkpoints/mit/index.md",
        "---\ntitle: MIT\n---\n",
    );
    put(
        &root,
        "phy/checkpoints/mit/2020.md",
        "---\ntitle: Physics, 2020\n---\n\nNo notes.\n\n\
         ::: exercise q1\nWhat is $I$?\n\n::: answer\n$V/R$.\n:::\n:::\n",
    );
    root
}

/// Read and check, returning every fault as a printed line.
fn faults(root: &Path) -> Vec<String> {
    let (node, mut found) = tree::read(root);
    if let Some(node) = node {
        found.extend(check::check(&node));
    }
    found.iter().map(ToString::to_string).collect()
}

#[test]
fn a_valid_repository_reports_nothing() {
    let root = valid("valid");
    assert_eq!(faults(&root), Vec::<String>::new());
}

/// The failure design C creates: a deck written and not added to `order`.
#[test]
fn a_deck_left_out_of_order_is_caught() {
    let root = valid("unlisted");
    put(
        &root,
        "phy/eam/fields/potential.md",
        "---\ntitle: Potential\n---\n\n::: card\nX\n:::\n",
    );
    let found = faults(&root);
    assert!(
        found
            .iter()
            .any(|f| f.contains("potential") && f.contains("does not list it")),
        "{found:?}"
    );
}

/// And `draft: true` is the way out of it. This test once asserted the
/// opposite of its own name and passed while `draft` did nothing at all.
#[test]
fn a_draft_is_not_caught() {
    let root = valid("draft");
    put(
        &root,
        "phy/eam/fields/potential.md",
        "---\ntitle: Potential\ndraft: true\n---\n",
    );
    let found = faults(&root);
    assert!(
        !found.iter().any(|f| f.contains("potential")),
        "a draft must not be caught, got {found:?}"
    );
}

/// A file that is not content does not become one. Practically every content
/// repository has a README beside its domains.
#[test]
fn a_readme_at_the_root_is_not_a_domain() {
    let root = valid("readme");
    put(&root, "README.md", "# The course\n");
    put(&root, "CONTRIBUTING.md", "# How to help\n");
    assert_eq!(faults(&root), Vec::<String>::new());
}

/// A domain declares the one language it is written in.
#[test]
fn a_domain_without_a_language_is_caught() {
    let root = valid("lang");
    put(
        &root,
        "phy/index.md",
        "---\ntitle: Physics\norder: [eam, reference]\n---\n",
    );
    let found = faults(&root);
    assert!(found.iter().any(|f| f.contains("BCP 47")), "{found:?}");
}

/// `order` naming one id twice is its own mistake, not a missing file.
#[test]
fn order_naming_one_id_twice_is_caught() {
    let root = valid("repeat");
    put(
        &root,
        "phy/eam/fields/index.md",
        "---\ntitle: Fields\norder: [field, field]\n---\n",
    );
    let found = faults(&root);
    assert!(
        found.iter().any(|f| f.contains("more than once")),
        "{found:?}"
    );
    assert!(
        !found.iter().any(|f| f.contains("no file or directory")),
        "a repeat must not read as a missing file, got {found:?}"
    );
}

#[test]
fn order_naming_a_file_that_is_not_there_is_caught() {
    let root = valid("missing");
    put(
        &root,
        "phy/index.md",
        "---\ntitle: Physics\nlang: en\norder: [eam, reference, mechanics]\n---\n",
    );
    let found = faults(&root);
    assert!(
        found
            .iter()
            .any(|f| f.contains("mechanics") && f.contains("no file")),
        "{found:?}"
    );
}

#[test]
fn a_major_version_this_build_cannot_read_is_refused() {
    let root = valid("major");
    put(&root, "index.md", "---\nformat: 1.0.0\norder: [phy]\n---\n");
    let found = faults(&root);
    assert!(found.iter().any(|f| f.contains("reads 2.x")), "{found:?}");
}

/// A minor bump is fine: the format may gain fields.
#[test]
fn a_newer_minor_is_accepted() {
    let root = valid("minor");
    put(&root, "index.md", "---\nformat: 2.9.4\norder: [phy]\n---\n");
    assert_eq!(faults(&root), Vec::<String>::new());
}

#[test]
fn a_block_outside_the_grammar_names_its_line() {
    let root = valid("grammar");
    put(
        &root,
        "phy/eam/fields/field.md",
        "---\ntitle: X\n---\n\n::: lesson\nX\n:::\n",
    );
    let found = faults(&root);
    assert!(
        found
            .iter()
            .any(|f| f.contains("field.md:5") && f.contains("`lesson` is not a block")),
        "{found:?}"
    );
}

#[test]
fn an_answer_cannot_stand_on_its_own() {
    let root = valid("orphan");
    put(
        &root,
        "phy/eam/fields/field.md",
        "---\ntitle: X\n---\n\n::: answer\nX\n:::\n",
    );
    let found = faults(&root);
    assert!(
        found.iter().any(|f| f.contains("cannot sit on its own")),
        "{found:?}"
    );
}

#[test]
fn an_oversized_asset_is_caught() {
    let root = valid("asset");
    put(&root, "phy/eam/fields/assets/big.png", &"x".repeat(300_000));
    let found = faults(&root);
    assert!(found.iter().any(|f| f.contains("over the")), "{found:?}");
}

#[test]
fn an_asset_of_the_wrong_type_is_caught() {
    let root = valid("asset-type");
    put(&root, "phy/eam/fields/assets/notes.txt", "x");
    let found = faults(&root);
    assert!(
        found.iter().any(|f| f.contains("not an asset")),
        "{found:?}"
    );
}

/// A deck and a reference may share a name: ids are unique per kind.
#[test]
fn a_deck_and_a_reference_may_share_a_name() {
    let root = valid("shared-name");
    put(
        &root,
        "phy/eam/fields/index.md",
        "---\ntitle: Fields\norder: [ohms-law]\n---\n",
    );
    std::fs::remove_file(root.join("phy/eam/fields/field.md")).expect("removes");
    put(&root, "phy/eam/fields/ohms-law.md", FIELD);
    assert_eq!(faults(&root), Vec::<String>::new());
}

#[test]
fn a_repository_with_no_root_index_says_so() {
    let root = std::env::temp_dir().join("stemin-format-noroot");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("makes the directory");
    let found = faults(&root);
    assert!(
        found
            .iter()
            .any(|f| f.contains("every content repository needs one")),
        "{found:?}"
    );
}

/// One bad file must not hide the faults in the files beside it.
#[test]
fn every_fault_is_reported_in_one_run() {
    let root = valid("all");
    put(&root, "index.md", "---\nformat: 3.0.0\norder: [phy]\n---\n");
    put(
        &root,
        "phy/eam/fields/potential.md",
        "---\ntitle: Potential\n---\n",
    );
    put(&root, "phy/eam/fields/assets/notes.txt", "x");
    let found = faults(&root);
    assert!(found.len() >= 3, "expected the whole list, got {found:?}");
}

/// A container can be unfinished exactly as a deck can. `draft` on a section
/// or a topic was parsed nowhere, so an author who marked one shipped it.
#[test]
fn a_draft_container_is_not_shipped() {
    let root = valid("draft-section");
    put(
        &root,
        "phy/eam/index.md",
        "---\ntitle: Electromagnetism\ndraft: true\norder: [fields]\n---\n",
    );
    let found = faults(&root);
    assert!(
        found.iter().any(|f| f.contains("marked `draft: true`")),
        "a drafted section named in `order` must say so, got {found:?}"
    );
}

/// A directory where only files belong can never be reached, so dropping it in
/// silence hides an author's mistake.
#[test]
fn a_directory_nested_too_deep_is_reported() {
    let root = valid("too-deep");
    put(
        &root,
        "phy/eam/fields/extra/note.md",
        "---\ntitle: Note\n---\n",
    );
    let found = faults(&root);
    assert!(
        found
            .iter()
            .any(|f| f.contains("nothing can ever reach it")),
        "{found:?}"
    );
}

/// A legend row that cannot split loses a line in the renderer, silently.
#[test]
fn a_malformed_legend_row_is_reported() {
    let root = valid("legend");
    put(
        &root,
        "phy/eam/fields/field.md",
        "---\ntitle: X\n---\n\n::: card\nX\n:::\n\n::: reference ohms-law\n# Ohm's law\n\n\
         ::: legend\n$I$: current\n$R$ resistance\n:::\n:::\n",
    );
    let found = faults(&root);
    assert!(
        found.iter().any(|f| f.contains("is not a legend row")),
        "{found:?}"
    );
}

/// The checker and the build read one reserved list. A deck named `test` used
/// to pass the checker and then be refused by the build.
#[test]
fn a_reserved_deck_id_is_caught_by_the_checker() {
    let root = valid("reserved");
    put(
        &root,
        "phy/eam/fields/index.md",
        "---\ntitle: Fields\norder: [test]\n---\n",
    );
    std::fs::remove_file(root.join("phy/eam/fields/field.md")).expect("removes");
    put(
        &root,
        "phy/eam/fields/test.md",
        "---\ntitle: T\n---\n\n::: card\nX\n:::\n",
    );
    let found = faults(&root);
    assert!(found.iter().any(|f| f.contains("is reserved")), "{found:?}");
}

/// The checker and the build must agree on where a block may sit. A deck
/// carrying an `::: equation` once passed `stemin check` and was then refused
/// by the compiler, which is the drift this crate exists to remove. An
/// equation now belongs inside its reference, and a checkpoint holds no
/// reference at all.
#[test]
fn a_block_in_the_wrong_kind_of_file_is_caught() {
    let root = valid("wrong-file");
    put(
        &root,
        "phy/eam/fields/field.md",
        "---\ntitle: X\n---\n\n::: equation\nE = m c^2\n:::\n",
    );
    put(
        &root,
        "phy/checkpoints/mit/2020.md",
        "---\ntitle: T\n---\n\n::: reference r\n# R\n:::\n",
    );
    let found = faults(&root);
    assert!(
        found
            .iter()
            .any(|f| f.contains("`equation` cannot sit on its own")),
        "{found:?}"
    );
    assert!(
        found
            .iter()
            .any(|f| f.contains("`reference` cannot sit inside `checkpoint`")),
        "{found:?}"
    );
}

/// A reference names itself twice: an id, which a link uses, and a title,
/// which the learner reads.
#[test]
fn a_reference_needs_an_id_and_a_title() {
    let root = valid("reference-name");
    put(
        &root,
        "phy/eam/fields/field.md",
        "---\ntitle: X\n---\n\n::: card\nX\n:::\n\n::: reference\n# No id\n:::\n\n\
         ::: reference no-title\nThe current is…\n:::\n",
    );
    let found = faults(&root);
    assert!(found.iter().any(|f| f.contains("needs an id")), "{found:?}");
    assert!(
        found
            .iter()
            .any(|f| f.contains("`no-title` has no `# Title` line")),
        "{found:?}"
    );
}

/// A `reference:` link reaches a reference from any deck of the domain, so two
/// decks may not both declare one id.
#[test]
fn two_decks_cannot_declare_one_reference() {
    let root = valid("reference-twice");
    put(
        &root,
        "phy/eam/fields/index.md",
        "---\ntitle: Fields\norder: [field, again]\n---\n",
    );
    put(&root, "phy/eam/fields/again.md", FIELD);
    let found = faults(&root);
    assert!(
        found
            .iter()
            .any(|f| f.contains("`ohms-law` is already the id of a reference")),
        "{found:?}"
    );
}

/// Checkpoints sit in folders to any depth, beside the sections and outside
/// the domain's `order`. Two folders may each hold a `2020`.
#[test]
fn checkpoints_nest_in_folders_and_may_share_an_id() {
    let root = valid("checkpoint-folders");
    put(
        &root,
        "phy/checkpoints/index.md",
        "---\norder: [mit, harvard]\n---\n",
    );
    put(
        &root,
        "phy/checkpoints/harvard/index.md",
        "---\ntitle: Harvard\norder: [finals]\n---\n",
    );
    put(
        &root,
        "phy/checkpoints/harvard/finals/index.md",
        "---\ntitle: Finals\n---\n",
    );
    put(
        &root,
        "phy/checkpoints/harvard/finals/2020.md",
        "---\ntitle: Finals, 2020\n---\n\n::: exercise\nQ\n\n::: answer\nA\n:::\n:::\n",
    );
    assert_eq!(faults(&root), Vec::<String>::new());
    let (node, _) = tree::read(&root);
    let node = node.expect("parses");
    let kinds: Vec<(String, tree::Kind)> = node
        .walk()
        .into_iter()
        .map(|n| (n.id.clone(), n.kind))
        .collect();
    assert!(kinds.contains(&("checkpoints".to_owned(), tree::Kind::CheckpointFolder)));
    assert!(kinds.contains(&("finals".to_owned(), tree::Kind::CheckpointFolder)));
    assert_eq!(
        kinds
            .iter()
            .filter(|(id, kind)| id == "2020" && *kind == tree::Kind::Checkpoint)
            .count(),
        2
    );
}

/// Format 1 kept its references in a folder of their own. That content does
/// not parse as format 2, and the checker says so rather than guessing.
#[test]
fn a_format_one_repository_is_refused() {
    let root = valid("format-one");
    put(&root, "index.md", "---\nformat: 1.0.0\norder: [phy]\n---\n");
    put(
        &root,
        "phy/index.md",
        "---\ntitle: Physics\nlang: en\norder: [eam, reference]\n---\n",
    );
    put(
        &root,
        "phy/reference/index.md",
        "---\norder: [ohms-law]\n---\n",
    );
    put(
        &root,
        "phy/reference/ohms-law.md",
        "---\ntitle: Ohm's law\n---\n\n::: equation\nI = V/R\n:::\n",
    );
    let found = faults(&root);
    assert!(found.iter().any(|f| f.contains("reads 2.x")), "{found:?}");
    assert!(
        found.iter().any(|f| f.contains("`reference` is reserved")),
        "{found:?}"
    );
}

/// An extension is not case-sensitive, and a dropped asset gives no signal.
#[test]
fn an_uppercase_asset_extension_is_still_an_asset() {
    let root = valid("upper-ext");
    put(&root, "phy/eam/fields/LOGO.PNG", "x");
    let found = faults(&root);
    assert!(
        found
            .iter()
            .any(|f| f.contains("sits outside an `assets/`")),
        "{found:?}"
    );
}

/// Whether one fault names a file and holds a phrase.
fn has(found: &[String], file: &str, phrase: &str) -> bool {
    found.iter().any(|f| f.contains(file) && f.contains(phrase))
}

/// An exercise with no answer has no back to its flashcard. The checker used
/// to pass it, and only the import refused it.
#[test]
fn an_exercise_without_an_answer_is_caught() {
    let root = valid("no-answer");
    put(
        &root,
        "phy/checkpoints/mit/2020.md",
        "---\ntitle: T\n---\n\n::: exercise q1\nWhat is $I$?\n:::\n",
    );
    let found = faults(&root);
    assert!(has(&found, "2020.md:5", "has no `::: answer`"), "{found:?}");
}

/// A misspelt key used to vanish in silence.
#[test]
fn an_unknown_front_matter_key_is_caught() {
    let root = valid("unknown-key");
    put(
        &root,
        "phy/eam/fields/index.md",
        "---\ntitle: Fields\nrequries: [x]\norder: [field]\n---\n",
    );
    put(
        &root,
        "phy/eam/fields/field.md",
        &FIELD.replace("title: What a field is", "title: X\nid: field"),
    );
    let found = faults(&root);
    assert!(
        has(
            &found,
            "fields/index.md",
            "`requries` is not a field of a topic: a topic takes title, tier, requires, order, draft"
        ),
        "{found:?}"
    );
    assert!(
        has(
            &found,
            "field.md",
            "`id` is not a field of a deck: a deck takes title, draft"
        ),
        "{found:?}"
    );
}

#[test]
fn an_unknown_label_is_caught() {
    let root = valid("unknown-label");
    put(
        &root,
        "phy/index.md",
        "---\ntitle: Physics\nlang: en\nlabels: { formula: Formula }\norder: [eam]\n---\n",
    );
    let found = faults(&root);
    assert!(
        has(&found, "phy/index.md", "`formula` is not a label"),
        "{found:?}"
    );
}

/// A deck's front matter was read only by the import, so bad YAML there
/// passed the checker.
#[test]
fn bad_yaml_in_a_deck_is_caught() {
    let root = valid("deck-yaml");
    put(
        &root,
        "phy/eam/fields/field.md",
        &FIELD.replace("title: What a field is", "title: [broken"),
    );
    let found = faults(&root);
    assert!(
        has(&found, "field.md", "front matter will not parse"),
        "{found:?}"
    );
}

/// Two exercises with one id would share one schedule. A derived `e2` counts.
#[test]
fn two_exercises_with_one_id_are_caught() {
    let root = valid("exercise-twice");
    put(
        &root,
        "phy/checkpoints/mit/2020.md",
        "---\ntitle: T\n---\n\n\
         ::: exercise e2\nQ\n::: answer\nA\n:::\n:::\n\n\
         ::: exercise\nQ\n::: answer\nA\n:::\n:::\n",
    );
    let found = faults(&root);
    assert!(
        has(
            &found,
            "2020.md:12",
            "`e2` is already the id of another exercise in this file, at line 5"
        ),
        "{found:?}"
    );
}

/// Text outside a block in a deck reaches no learner.
#[test]
fn loose_text_in_a_deck_is_caught() {
    let root = valid("loose-text");
    put(
        &root,
        "phy/eam/fields/field.md",
        &FIELD.replace("::: card\n", "A stray line.\n\n::: card\n"),
    );
    let found = faults(&root);
    assert!(
        has(&found, "field.md:5", "sits outside any block"),
        "{found:?}"
    );
}

/// The app draws a plot in a card or a figure, one a card, and drops every
/// other one.
#[test]
fn a_plot_the_app_never_draws_is_caught() {
    let root = valid("plot-place");
    let plot = "```plot\nx: { var: t, from: 0, to: 1 }\ny: { from: 0, to: 1 }\n```\n";
    put(
        &root,
        "phy/eam/fields/field.md",
        &format!(
            "---\ntitle: X\n---\n\n::: card\n{plot}\n{plot}:::\n\n\
             ::: exercise\nQ\n\n{plot}\n::: answer\nA\n:::\n:::\n"
        ),
    );
    let found = faults(&root);
    assert!(
        has(&found, "field.md:11", "already holds a `plot` block"),
        "{found:?}"
    );
    assert!(
        has(&found, "field.md:20", "this one sits in `exercise`"),
        "{found:?}"
    );
}

/// A topic picks its tier from its section's list.
#[test]
fn a_tier_the_section_does_not_list_is_caught() {
    let root = valid("tier");
    put(
        &root,
        "phy/eam/index.md",
        "---\ntitle: Electromagnetism\ntiers: [entrance]\norder: [fields]\n---\n",
    );
    put(
        &root,
        "phy/eam/fields/index.md",
        "---\ntitle: Fields\ntier: graduate\norder: [field]\n---\n",
    );
    let found = faults(&root);
    assert!(
        has(
            &found,
            "fields/index.md",
            "tier `graduate` is not in the section's `tiers`, [entrance]"
        ),
        "{found:?}"
    );
}

/// A `requires` entry names a topic that is there, inside this repository,
/// and the graph has no cycle.
#[test]
fn a_broken_requires_is_caught() {
    let root = valid("requires");
    put(
        &root,
        "phy/eam/index.md",
        "---\ntitle: Electromagnetism\norder: [fields, circuits]\n---\n",
    );
    put(
        &root,
        "phy/eam/fields/index.md",
        "---\ntitle: Fields\nrequires: [circuits, nothing, /chem/moles/mole, /phy/eam/gone]\n\
         order: [field]\n---\n",
    );
    put(
        &root,
        "phy/eam/circuits/index.md",
        "---\ntitle: Circuits\nrequires: [/phy/eam/fields]\norder: [loop]\n---\n",
    );
    put(
        &root,
        "phy/eam/circuits/loop.md",
        "---\ntitle: A loop\n---\n\n::: card\nX\n:::\n",
    );
    let found = faults(&root);
    let at = "fields/index.md";
    assert!(
        has(&found, at, "`nothing` names no topic in this section"),
        "{found:?}"
    );
    assert!(
        has(
            &found,
            at,
            "`/chem/moles/mole` points outside this repository"
        ),
        "{found:?}"
    );
    assert!(
        has(&found, at, "`/phy/eam/gone` names no topic in domain `phy`"),
        "{found:?}"
    );
    assert!(
        has(
            &found,
            "circuits/index.md",
            "`requires` is cyclic: /phy/eam/circuits -> /phy/eam/fields -> /phy/eam/circuits"
        ),
        "{found:?}"
    );
}

/// The audit's M6 probes: a link to a file outside the repository, and two
/// links that each point at their own directory. The walk follows neither, so
/// it reads nothing outside the repository and ends.
#[cfg(unix)]
#[test]
fn a_symbolic_link_is_refused_and_never_followed() {
    let root = valid("symlink");
    let fields = root.join("phy/eam/fields");
    for (target, name) in [("/etc/passwd", "passwd.md"), (".", "a"), (".", "b")] {
        std::os::unix::fs::symlink(target, fields.join(name)).expect("links");
    }
    let found = faults(&root);
    for name in ["passwd.md", "a", "b"] {
        assert!(
            found
                .iter()
                .any(|f| f.contains(&format!("`{name}` is a symbolic link"))),
            "{found:?}"
        );
    }
    assert!(!found.iter().any(|f| f.contains("root:")), "{found:?}");
}

/// A `.md` file past the text cap is refused by its size, and never parsed.
#[test]
fn a_text_file_past_its_cap_is_refused() {
    let root = valid("text-cap");
    let card = "::: card\nA line of prose that fills the deck.\n:::\n\n";
    let cap = usize::try_from(tree::TEXT_CAP).expect("fits");
    let body = card.repeat(cap.div_ceil(card.len()));
    put(
        &root,
        "phy/eam/fields/field.md",
        &format!("---\ntitle: Long\n---\n\n{body}"),
    );
    let found = faults(&root);
    assert!(
        has(
            &found,
            "fields/field.md",
            "over the 524288-byte cap on a `.md` file"
        ),
        "{found:?}"
    );
}

/// The audit's M3 probe: checkpoint folders nested past the depth cap. The
/// walk reads nothing below it, so no walk recurses without a bound.
#[test]
fn a_path_past_the_depth_cap_is_refused() {
    let root = valid("depth");
    let mut dir = "phy/checkpoints/mit".to_owned();
    for _ in 0..tree::MAX_DEPTH {
        put(
            &root,
            &format!("{dir}/index.md"),
            "---\ntitle: Deeper\n---\n",
        );
        dir.push_str("/a");
    }
    put(
        &root,
        &format!("{dir}/index.md"),
        "---\ntitle: Deepest\n---\n",
    );
    let found = faults(&root);
    assert!(
        found
            .iter()
            .any(|f| f.contains("levels below the repository root")),
        "{found:?}"
    );
}

/// The audit's M2 probe: blocks nested thousands deep. The parse refuses the
/// first block that opens two levels down, at its line.
#[test]
fn blocks_nested_too_deep_are_refused_at_their_line() {
    let root = valid("nesting");
    put(
        &root,
        "phy/eam/fields/field.md",
        &format!("---\ntitle: X\n---\n\n{}", "::: card\n".repeat(20_000)),
    );
    let found = faults(&root);
    assert!(
        has(&found, "fields/field.md:7", "opens two blocks deep"),
        "{found:?}"
    );
}
