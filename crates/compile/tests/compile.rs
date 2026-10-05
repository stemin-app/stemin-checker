//! Compiling a repository the way the browser does it.
//!
//! The worker holds a repository as paths and bytes it fetched from a forge,
//! walks it with `tree::read_map`, and compiles it. These tests take that exact
//! path, natively, because the browser is where it runs and the browser is
//! where it cannot be tested.

#![expect(
    clippy::expect_used,
    reason = "a fixture that will not compile is a broken test, and must stop it loudly"
)]

use stemin_compile::compile;
use stemin_compile::model::Bundle;
use stemin_format::tree::{Blob, Files};

/// The URL every test repository is added by, so the ids are the ids a learner
/// would get.
const URL: &str = "https://github.com/author/course";

/// A repository as the browser holds one.
fn repo(pairs: &[(&str, &str)]) -> Files {
    pairs
        .iter()
        .map(|(path, body)| ((*path).to_owned(), Blob::new((*body).into())))
        .collect()
}

/// The one reference of the small repository, as its deck declares it.
const OHMS_LAW: &str = "::: reference ohms-law\n# Ohm's law\n\n\
                        Current is voltage over resistance.\n\n\
                        ::: equation\nI = \\frac{V}{R}\n:::\n\n\
                        ::: legend\n$I$: current, in amperes\n:::\n:::\n";

/// One small, valid repository: one domain, one section, one topic, one deck
/// with one reference.
fn small() -> Files {
    let deck = format!(
        "---\ntitle: What a field is\n---\n\n\
         ::: card\nA field is an influence at every point, $E = F/q$.\n\
         See [Ohm's law](reference:ohms-law).\n:::\n\n\
         ::: exercise q1\nWhat does a field act on?\n\n\
         ::: answer\nCharge.\n:::\n:::\n\n{OHMS_LAW}"
    );
    let mut files = repo(&[
        (
            "index.md",
            "---\nformat: 2.0.0\nauthor: A Name\ndescription: A course.\norder: [phy]\n---\n",
        ),
        (
            "phy/index.md",
            "---\ntitle: Physics\nlang: en\norder: [eam]\n---\n",
        ),
        (
            "phy/eam/index.md",
            "---\ntitle: Electromagnetism\ntiers: [entrance]\norder: [fields]\n---\n",
        ),
        (
            "phy/eam/fields/index.md",
            "---\ntitle: Fields\ntier: entrance\norder: [field]\n---\n",
        ),
    ]);
    files.insert("phy/eam/fields/field.md".to_owned(), Blob::new(deck.into()));
    files
}

/// Walk and compile, as the worker does.
fn build(files: &Files) -> Vec<Bundle> {
    let (root, faults) = stemin_format::tree::read_map("", files);
    let root = root.expect("the repository walks");
    assert!(faults.is_empty(), "{faults:?}");
    compile::compile(&root, URL, files).expect("the repository compiles")
}

#[test]
fn a_repository_compiles_to_one_bundle_per_domain() {
    let bundles = build(&small());
    assert_eq!(bundles.len(), 1);
    let bundle = &bundles[0];
    assert_eq!(bundle.domain.title, "Physics");
    assert_eq!(bundle.lang, "en");
    // The domain names no author, so it takes the repository's: the selector
    // reads "by A Name" for every domain, not only for the ones that repeat it.
    assert_eq!(bundle.author.as_deref(), Some("A Name"));
    assert_eq!(bundle.decks.len(), 1);
    assert_eq!(bundle.references.len(), 1);
}

/// Every id is under the domain's **hash**, never its directory name: two
/// repositories that both hold a `phy` must never share a progress row.
#[test]
fn every_id_sits_under_the_domain_hash() {
    let bundles = build(&small());
    let hash = stemin_format::id::domain_id(URL, "phy");
    let deck = bundles[0].decks.keys().next().expect("one deck");
    assert_eq!(deck.as_str(), format!("{hash}/eam/fields/field"));
    let reference = bundles[0].references.keys().next().expect("one reference");
    assert_eq!(reference.as_str(), format!("{hash}/ohms-law"));
    assert_eq!(bundles[0].domain.id, hash);
}

/// The card's prose, its math and its reference mark all land in the HTML.
#[test]
fn a_card_renders_to_self_contained_html() {
    let bundles = build(&small());
    let deck = bundles[0].decks.values().next().expect("one deck");
    let card = &deck.cards[0];
    assert!(card.html.contains("<p>"), "{}", card.html);
    assert!(card.html.contains("<math"), "{}", card.html);
    assert!(card.html.contains("reference-link"), "{}", card.html);
    assert!(!card.rev.is_empty());
    // The exercise came through with its answer.
    assert_eq!(deck.exercises.len(), 1);
    assert!(deck.exercises[0].answer.contains("Charge"));
}

/// An asset is inlined: a picture is part of the card, and nothing is fetched
/// at runtime.
#[test]
fn an_svg_asset_is_re_emitted_into_the_card() {
    let mut files = small();
    files.insert(
        "phy/eam/fields/field.md".to_owned(),
        Blob::new(
            "---\ntitle: What a field is\n---\n\n\
             ::: card\n![A line](assets/line.svg)\n:::\n\n\
             ::: exercise q1\nWhy?\n\n::: answer\nBecause.\n:::\n:::\n"
                .into(),
        ),
    );
    files.insert(
        "phy/eam/fields/assets/line.svg".to_owned(),
        Blob::new(
            r##"<svg viewBox="0 0 4 4"><script>alert(1)</script><line x1="0" y1="0" x2="4" y2="4" stroke="#000000"/></svg>"##
                .into(),
        ),
    );
    let bundles = build(&files);
    let deck = bundles[0].decks.values().next().expect("one deck");
    let html = &deck.cards[0].html;
    assert!(html.contains("<svg class=\"asset\""), "{html}");
    assert!(html.contains("stroke=\"currentColor\""), "{html}");
    // The author's script is not in the markup, and neither is its body.
    assert!(!html.contains("script"), "{html}");
    assert!(!html.contains("alert"), "{html}");
}

/// A `plot` block leaves the prose and rides on the card as a spec the app
/// draws live.
#[test]
fn a_plot_block_becomes_a_spec() {
    let mut files = small();
    files.insert(
        "phy/eam/fields/field.md".to_owned(),
        Blob::new(
            "---\ntitle: What a field is\n---\n\n\
             ::: card\nThe field falls off with distance.\n\n\
             ```plot\nx: { var: r, label: \"$r$\", from: 1, to: 10 }\n\
             y: { label: \"$E$\", from: 0, to: 1 }\n\
             inputs:\n  - { name: q, min: 1, max: 4, default: 1, step: 1 }\n\
             draw:\n  - curve: { is: q / (r * r), accent: true }\n```\n:::\n\n\
             ::: exercise q1\nWhy?\n\n::: answer\nBecause.\n:::\n:::\n"
                .into(),
        ),
    );
    let bundles = build(&files);
    let deck = bundles[0].decks.values().next().expect("one deck");
    let plot = deck.cards[0]
        .plot
        .as_ref()
        .expect("the card carries a plot");
    assert_eq!(plot.var(), "r");
    assert_eq!(plot.inputs.len(), 1);
    assert_eq!(plot.draw.len(), 1);
    assert!(!deck.cards[0].html.contains("curve"));
}

/// A reference into another domain of the **same** repository resolves to that
/// domain's hash; one that names a domain the repository does not hold is
/// refused, rather than emitted as a dangling edge.
#[test]
fn a_cross_domain_reference_resolves_and_a_foreign_one_does_not() {
    let mut files = small();
    files.insert(
        "index.md".to_owned(),
        Blob::new("---\nformat: 2.0.0\norder: [phy, math]\n---\n".into()),
    );
    files.insert(
        "math/index.md".to_owned(),
        Blob::new("---\ntitle: Mathematics\nlang: en\norder: [alg]\n---\n".into()),
    );
    files.insert(
        "math/alg/index.md".to_owned(),
        Blob::new("---\ntitle: Algebra\norder: [linears]\n---\n".into()),
    );
    files.insert(
        "math/alg/linears/index.md".to_owned(),
        Blob::new("---\ntitle: Linear equations\norder: [solving]\n---\n".into()),
    );
    files.insert(
        "math/alg/linears/solving.md".to_owned(),
        Blob::new(
            "---\ntitle: Solving\n---\n\n::: card\nIsolate the unknown.\n:::\n\n\
             ::: exercise q1\nSolve it.\n\n::: answer\n$x = 3$.\n:::\n:::\n\n\
             ::: reference intercept\n# The intercept\n\nIt meets the axis at $b$.\n:::\n"
                .into(),
        ),
    );
    // The physics reference now rests on the mathematics one, across domains:
    // its derivation links it.
    let rests = |target: &str| {
        format!(
            "---\ntitle: What a field is\n---\n\n::: card\nA field.\n:::\n\n\
             ::: reference ohms-law\n# Ohm's law\n\nCurrent is voltage over resistance.\n\n\
             ::: derivation\nAs [the intercept]({target}) shows. ∎\n:::\n:::\n"
        )
    };
    files.insert(
        "phy/eam/fields/field.md".to_owned(),
        Blob::new(rests("reference:/math/intercept").into()),
    );

    let bundles = build(&files);
    let math = stemin_format::id::domain_id(URL, "math");
    let edge = bundles
        .iter()
        .flat_map(|bundle| &bundle.edges)
        .find(|(_, kind, _)| *kind == stemin_compile::model::EdgeKind::Uses)
        .expect("the `uses` edge is there");
    assert_eq!(edge.2.as_str(), format!("{math}/intercept"));

    // A reference to a domain this repository does not hold is refused.
    files.insert(
        "phy/eam/fields/field.md".to_owned(),
        Blob::new(rests("reference:/chem/moles").into()),
    );
    let (root, _) = stemin_format::tree::read_map("", &files);
    let root = root.expect("walks");
    let refused = compile::compile(&root, URL, &files).expect_err("a foreign reference is refused");
    assert!(refused.to_string().contains("chem"), "{refused}");
}

/// The same repository at another URL is another set of domains, and the two
/// never share a progress row.
#[test]
fn two_repositories_never_collide() {
    let files = small();
    let mine = build(&files);
    let (root, _) = stemin_format::tree::read_map("", &files);
    let theirs = compile::compile(
        &root.expect("walks"),
        "https://codeberg.org/someone/course",
        &files,
    )
    .expect("compiles");
    assert_ne!(mine[0].domain.id, theirs[0].domain.id);
}

/// Content the checker rejects never reaches a bundle: the compiler checks
/// first, and reports **every** fault rather than the first.
#[test]
fn invalid_content_is_refused_with_every_fault() {
    let mut files = small();
    // A deck that `order` does not name, and a reserved id.
    files.insert(
        "phy/eam/fields/practice.md".to_owned(),
        Blob::new("---\ntitle: Practice\n---\n\n::: card\nx\n:::\n".into()),
    );
    let (root, _) = stemin_format::tree::read_map("", &files);
    let error = compile::compile(&root.expect("walks"), URL, &files)
        .expect_err("the repository is refused");
    let message = error.to_string();
    assert!(message.contains("practice"), "{message}");
}

/// An author's repository holds a README and a licence beside the content, and
/// neither is content.
#[test]
fn a_readme_beside_the_domains_is_not_a_domain() {
    let mut files = small();
    files.insert(
        "README.md".to_owned(),
        Blob::new("# A course\n\nRead me.\n".into()),
    );
    files.insert("LICENSE.md".to_owned(), Blob::new("MIT.\n".into()));
    let bundles = build(&files);
    assert_eq!(bundles.len(), 1, "one domain, not three");
}

/// The bundle round-trips through JSON, which is how it crosses from the
/// worker to the device and back out of `IndexedDB`.
#[test]
fn a_bundle_round_trips_through_json() {
    let bundles = build(&small());
    let json = serde_json::to_string(&bundles[0]).expect("serialises");
    let back: Bundle = serde_json::from_str(&json).expect("reads back");
    assert_eq!(back.domain.id, bundles[0].domain.id);
    assert_eq!(back.decks.len(), bundles[0].decks.len());
    assert_eq!(back.references.len(), bundles[0].references.len());
}

/// A plot survives the same round trip, single-key map and all, so a card that
/// moves is a card that still draws.
#[test]
fn a_plot_round_trips_through_json_and_yaml() {
    let source = "x: { var: t, from: 0, to: 1 }\ny: { from: 0, to: 1 }\n\
                  let:\n  w: 2 * pi()\n\
                  draw:\n  - curve: { is: sin(w * t), accent: true }\n  - hline: 0.5\n";
    let plot = stemin_format::plot::parse(source).expect("parses");
    for text in [
        serde_json::to_string(&plot).expect("json"),
        serde_yaml::to_string(&plot).expect("yaml"),
    ] {
        let back = stemin_format::plot::parse(&text)
            .or_else(|_| serde_json::from_str(&text).map_err(|err| err.to_string()))
            .expect("reads back");
        assert_eq!(back.draw.len(), 2);
        assert_eq!(back.lets.len(), 1);
        assert_eq!(back.var(), "t");
    }
}

/// A corrected answer is a different question to the scheduler, so the
/// exercise's rev moves with it and the refresh clears its schedule.
#[test]
fn an_edited_answer_moves_the_rev() {
    let before = build(&small());
    let mut files = small();
    let deck = "phy/eam/fields/field.md";
    let edited = files
        .get(deck)
        .and_then(Blob::text)
        .map(|text| text.replace("Charge.", "Electric charge."))
        .expect("the deck is there");
    files.insert(deck.to_owned(), Blob::new(edited.into()));
    let after = build(&files);
    let rev = |bundles: &[Bundle]| {
        bundles[0]
            .decks
            .values()
            .next()
            .map(|deck| deck.exercises[0].rev.clone())
            .expect("one exercise")
    };
    assert_ne!(rev(&before), rev(&after));
}

/// A figure id is the author's text, and it reaches the card only escaped.
#[test]
fn a_figure_id_cannot_become_markup() {
    let mut files = small();
    files.insert(
        "phy/eam/fields/field.md".to_owned(),
        Blob::new(
            "---\ntitle: A field\n---\n\n\
             ::: card\nSee [it](<figure:a\"onmouseover=\"x>).\n:::\n\n\
             ::: figure a\"onmouseover=\"x\n![A line](assets/line.svg)\n\nA caption.\n:::\n\n\
             ::: exercise q1\nWhy?\n\n::: answer\nBecause.\n:::\n:::\n"
                .into(),
        ),
    );
    files.insert(
        "phy/eam/fields/assets/line.svg".to_owned(),
        Blob::new(r#"<svg viewBox="0 0 4 4"><line x1="0" y1="0" x2="4" y2="4"/></svg>"#.into()),
    );
    let bundles = build(&files);
    let deck = bundles[0].decks.values().next().expect("one deck");
    let html = &deck.cards[0].html;
    assert!(html.contains("figure-ref"), "{html}");
    // The quote is escaped, so the attribute cannot close early.
    assert!(!html.contains("\"onmouseover"), "{html}");
}

/// A deck declares its references, in order, and the bundle holds each one
/// under the domain, where a `reference:` link from any deck reaches it.
#[test]
fn a_deck_declares_its_own_references() {
    let bundles = build(&small());
    let bundle = &bundles[0];
    let domain = &bundle.domain.id;
    let (_, deck) = bundle.decks.iter().next().expect("one deck");
    assert_eq!(
        deck.references
            .iter()
            .map(|id| id.as_str().to_owned())
            .collect::<Vec<_>>(),
        vec![format!("{domain}/ohms-law")]
    );
    let ohms = bundle
        .references
        .get(&deck.references[0])
        .expect("the reference is in the bundle");
    assert_eq!(ohms.title, "Ohm's law");
    assert!(ohms.reference.contains("<math"), "{}", ohms.reference);
    assert!(ohms.html.contains("Current is voltage"), "{}", ohms.html);
    assert!(
        !ohms.html.contains("Ohm's law"),
        "the title is not prose: {}",
        ohms.html
    );
    assert_eq!(ohms.legend.len(), 1);
}

/// Checkpoints compile as a tree of folders, each checkpoint with its own
/// questions. Nothing about them enters a deck.
#[test]
fn checkpoints_compile_as_a_tree_of_folders() {
    use stemin_compile::model::CheckpointEntry;
    let mut files = small();
    for (path, body) in [
        ("phy/checkpoints/index.md", "---\norder: [mit]\n---\n"),
        ("phy/checkpoints/mit/index.md", "---\ntitle: MIT\n---\n"),
        (
            "phy/checkpoints/mit/2020.md",
            "---\ntitle: Physics, 2020\n---\n\nNo notes.\n\n\
             ::: exercise q1\nWhat is $I$?\n\n::: answer\n$V/R$.\n:::\n\n\
             ::: solution\nBy [Ohm's law](reference:ohms-law).\n:::\n:::\n",
        ),
    ] {
        files.insert(path.to_owned(), Blob::new(body.into()));
    }
    let bundles = build(&files);
    let bundle = &bundles[0];
    let [CheckpointEntry::Folder(mit)] = bundle.checkpoints.as_slice() else {
        panic!("one folder: {:?}", bundle.checkpoints);
    };
    assert_eq!(mit.title, "MIT");
    let [CheckpointEntry::Checkpoint(exam)] = mit.entries.as_slice() else {
        panic!("one checkpoint: {:?}", mit.entries);
    };
    assert_eq!(exam.title, "Physics, 2020");
    assert!(exam.html.contains("No notes"), "{}", exam.html);
    assert_eq!(exam.questions.len(), 1);
    assert!(exam.questions[0].solution.is_some());
    // A checkpoint's question is not a deck's exercise.
    assert_eq!(
        bundle
            .decks
            .values()
            .map(|d| d.exercises.len())
            .sum::<usize>(),
        1
    );
    // The bundle round-trips with its tree.
    let json = serde_json::to_string(bundle).expect("serialises");
    let back: Bundle = serde_json::from_str(&json).expect("parses");
    assert_eq!(back.checkpoints.len(), 1);
}

/// An asset may sit at any level, and a deck reaches one above it.
#[test]
fn a_deck_inlines_an_asset_from_a_level_above() {
    let mut files = small();
    files.insert(
        "phy/eam/assets/dot.svg".to_owned(),
        Blob::new(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 10 10\">\
             <circle cx=\"5\" cy=\"5\" r=\"4\"/></svg>"
                .into(),
        ),
    );
    files.insert(
        "phy/eam/fields/field.md".to_owned(),
        Blob::new(
            "---\ntitle: What a field is\n---\n\n::: card\n![A dot](../assets/dot.svg)\n:::\n"
                .into(),
        ),
    );
    let bundles = build(&files);
    let (_, deck) = bundles[0].decks.iter().next().expect("one deck");
    assert!(
        deck.cards[0].html.contains("<circle"),
        "{}",
        deck.cards[0].html
    );
}

/// Walk and check, as `stemin check` does: every fault, as a printed line.
fn faults(files: &Files) -> Vec<String> {
    let (root, walked) = stemin_format::tree::read_map("", files);
    let root = root.expect("the repository walks");
    assert!(walked.is_empty(), "{walked:?}");
    compile::check(&root, URL, files)
        .iter()
        .map(ToString::to_string)
        .collect()
}

/// The small repository with its one deck replaced.
fn with_deck(deck: &str) -> Files {
    let mut files = small();
    files.insert(
        "phy/eam/fields/field.md".to_owned(),
        Blob::new(format!("---\ntitle: A field\n---\n\n{deck}\n{OHMS_LAW}").into()),
    );
    files
}

/// Whether one fault holds every phrase.
fn has(found: &[String], phrases: &[&str]) -> bool {
    found
        .iter()
        .any(|fault| phrases.iter().all(|phrase| fault.contains(phrase)))
}

/// The LaTeX renderer draws a red box for bad LaTeX and carries on. The
/// learner would see that box, so the import refuses it, with its line.
#[test]
fn bad_latex_is_refused_at_its_line() {
    let found = faults(&with_deck(
        "::: card\nFine $x$.\n\nBroken $\\frac{a}$.\n:::\n",
    ));
    assert!(
        has(
            &found,
            &[
                "phy/eam/fields/field.md:8:",
                "the LaTeX `\\frac{a}` will not render: parsing error: expected a token"
            ]
        ),
        "{found:?}"
    );
}

/// An equation and a legend row are LaTeX too.
#[test]
fn bad_latex_in_a_reference_is_refused() {
    let mut files = small();
    files.insert(
        "phy/eam/fields/field.md".to_owned(),
        Blob::new(
            "---\ntitle: A field\n---\n\n::: card\nX\n:::\n\n\
             ::: reference ohms-law\n# Ohm's law\n\n::: equation\n\\frac{V}\n:::\n\n\
             ::: legend\n$\\nosuch$: a symbol\n:::\n:::\n"
                .into(),
        ),
    );
    let found = faults(&files);
    assert!(
        has(&found, &["field.md:13:", "will not render"]),
        "{found:?}"
    );
    assert!(has(&found, &["field.md:17:", "`\\nosuch`"]), "{found:?}");
}

/// A link to no reference fails with its file and line, wherever it sits: an
/// answer too, which once added no edge and so was never checked.
#[test]
fn a_dangling_link_is_refused_even_in_an_answer() {
    let found = faults(&with_deck(
        "::: card\nSee [it](reference:nope).\n:::\n\n\
         ::: exercise q1\nWhy?\n\n::: answer\nBy [this](reference:gone).\n:::\n:::\n",
    ));
    assert!(
        has(
            &found,
            &[
                "field.md:6:",
                "`reference:nope` names no reference in this domain"
            ]
        ),
        "{found:?}"
    );
    assert!(
        has(
            &found,
            &[
                "field.md:13:",
                "`reference:gone` names no reference in this domain"
            ]
        ),
        "{found:?}"
    );
}

/// A link in an answer is a `reference` edge from its exercise, as a link in
/// its prompt is.
#[test]
fn a_link_in_an_answer_is_an_edge() {
    let bundles = build(&with_deck(
        "::: card\nX\n:::\n\n\
         ::: exercise q1\nWhy?\n\n::: answer\nBy [Ohm's law](reference:ohms-law).\n:::\n:::\n",
    ));
    let edge = bundles[0]
        .edges
        .iter()
        .find(|(from, _, _)| from.as_str().ends_with("#q1"))
        .expect("the answer's link is an edge");
    assert_eq!(edge.1, stemin_compile::model::EdgeKind::Reference);
    assert!(edge.2.as_str().ends_with("/ohms-law"), "{edge:?}");
}

/// A link into another domain of the repository must name a reference that
/// is there. It was never checked: only links inside one domain were.
#[test]
fn a_dangling_link_into_another_domain_is_refused() {
    let mut files = small();
    files.insert(
        "index.md".to_owned(),
        Blob::new("---\nformat: 2.0.0\norder: [phy, math]\n---\n".into()),
    );
    for (path, body) in [
        (
            "math/index.md",
            "---\ntitle: Mathematics\nlang: en\norder: [alg]\n---\n",
        ),
        (
            "math/alg/index.md",
            "---\ntitle: Algebra\norder: [linears]\n---\n",
        ),
        (
            "math/alg/linears/index.md",
            "---\ntitle: Linear\norder: [solving]\n---\n",
        ),
        (
            "math/alg/linears/solving.md",
            "---\ntitle: Solving\n---\n\n::: card\nSee [it](reference:/phy/nope).\n:::\n",
        ),
    ] {
        files.insert(path.to_owned(), Blob::new(body.into()));
    }
    let found = faults(&files);
    assert!(
        has(
            &found,
            &[
                "math/alg/linears/solving.md:6:",
                "`reference:/phy/nope` names no reference in domain `phy`"
            ]
        ),
        "{found:?}"
    );
}

/// A `figure:` link names a figure of its own deck.
#[test]
fn a_dangling_figure_link_is_refused() {
    let found = faults(&with_deck("::: card\nAs [it](figure:nope) shows.\n:::\n"));
    assert!(
        has(
            &found,
            &["field.md:6:", "`figure:nope` names no figure in this deck"]
        ),
        "{found:?}"
    );
}

/// A picture that is not there, and an SVG the allowlist cannot read.
#[test]
fn a_missing_or_broken_picture_is_refused() {
    let mut files = with_deck(
        "::: card\n![None](assets/none.png)\n:::\n\n::: card\n![Broken](assets/b.svg)\n:::\n",
    );
    files.insert(
        "phy/eam/fields/assets/b.svg".to_owned(),
        Blob::new("not markup at all".into()),
    );
    let found = faults(&files);
    assert!(
        has(
            &found,
            &["field.md:6:", "`assets/none.png` is not in this repository"]
        ),
        "{found:?}"
    );
    assert!(
        has(&found, &["field.md:10:", "`phy/eam/fields/assets/b.svg`"]),
        "{found:?}"
    );
}

/// The references a derivation links are what it rests on, and that graph
/// has no cycle.
#[test]
fn a_uses_cycle_is_refused() {
    let mut files = small();
    files.insert(
        "phy/eam/fields/field.md".to_owned(),
        Blob::new(
            "---\ntitle: A field\n---\n\n::: card\nX\n:::\n\n\
             ::: reference a\n# A\n\n::: derivation\nFrom [b](reference:b).\n:::\n:::\n\n\
             ::: reference b\n# B\n\n::: derivation\nFrom [a](reference:a).\n:::\n:::\n"
                .into(),
        ),
    );
    let found = faults(&files);
    assert!(
        has(
            &found,
            &[
                "field.md:9:",
                "`uses` is cyclic: /phy/a -> /phy/b -> /phy/a"
            ]
        ),
        "{found:?}"
    );
}

/// One run names every fault, not the first: the import's message and the
/// checker's list are the same list.
#[test]
fn the_compile_reports_every_fault_at_once() {
    let files = with_deck(
        "::: card\n$\\frac{a}$ and [x](reference:nope) and ![p](assets/none.png).\n:::\n",
    );
    let (root, _) = stemin_format::tree::read_map("", &files);
    let error = compile::compile(&root.expect("walks"), URL, &files)
        .expect_err("the repository is refused");
    let message = error.to_string();
    assert!(message.contains("will not render"), "{message}");
    assert!(message.contains("reference:nope"), "{message}");
    assert!(message.contains("assets/none.png"), "{message}");
}

/// The audit's C1 probes, through the whole compile: in a card, in a
/// reference's equation, and in a legend symbol. No element the LaTeX names
/// reaches the bundle.
#[test]
fn markup_inside_latex_never_reaches_a_bundle() {
    let payloads = [
        r"\text{<img src=x onerror=alert(1)>}",
        r"\operatorname{<b>x</b>}",
        r"\text{<style>body{}</style>}",
        r#"\text{<meta http-equiv="refresh" content="0;url=https://evil.example/">}"#,
        r"\text{&lt;script&gt;}",
    ];
    for payload in payloads {
        // A legend row splits at its first `:`, so a payload with one is a
        // symbol in the equation alone.
        let symbol = if payload.contains(':') { "x" } else { payload };
        let mut files = small();
        files.insert(
            "phy/eam/fields/field.md".to_owned(),
            Blob::new(
                format!(
                    "---\ntitle: A field\n---\n\n::: card\nInline ${payload}$.\n\n$$ {payload} $$\n:::\n\n\
                     ::: reference ohms-law\n# Ohm's law\n\n::: equation\n{payload}\n:::\n\n\
                     ::: legend\n${symbol}$: a symbol\n:::\n:::\n"
                )
                .into(),
            ),
        );
        let bundles = build(&files);
        let json = serde_json::to_string(&bundles).expect("serialises");
        for gone in ["<img", "<style", "<meta", "<b>", "<script"] {
            assert!(!json.contains(gone), "{payload}: {json}");
        }
    }
}

/// The audit's M5 probe: one picture shown many times. Each place inlines
/// it, so the domain would grow without a bound; the compile refuses it once
/// it passes the cap, and stops inlining.
#[test]
fn a_picture_shown_too_often_is_refused() {
    let mut files = small();
    let side = usize::try_from(stemin_format::tree::ASSET_CAP).expect("fits");
    files.insert(
        "phy/eam/fields/assets/big.png".to_owned(),
        Blob::new(vec![0; side]),
    );
    let shown = "![big](assets/big.png)\n\n".repeat(200);
    files.insert(
        "phy/eam/fields/field.md".to_owned(),
        Blob::new(format!("---\ntitle: A field\n---\n\n::: card\n{shown}:::\n").into()),
    );
    let found = faults(&files);
    assert!(
        has(
            &found,
            &["field.md", "passes its cap of 32 MB in this file"]
        ),
        "{found:?}"
    );
    assert_eq!(found.len(), 1, "{found:?}");
}

/// The compile holds an asset to its cap on its own, whatever map of files it
/// is given, not only after the walk.
#[test]
fn the_compile_holds_an_asset_to_its_cap() {
    let mut files = small();
    files.insert(
        "phy/eam/fields/assets/huge.png".to_owned(),
        Blob {
            bytes: vec![0; 16],
            size: stemin_format::tree::ASSET_CAP + 1,
        },
    );
    files.insert(
        "phy/eam/fields/field.md".to_owned(),
        Blob::new("---\ntitle: A field\n---\n\n::: card\n![huge](assets/huge.png)\n:::\n".into()),
    );
    let (root, walked) = stemin_format::tree::read_map("", &files);
    assert!(!walked.is_empty(), "the walk refuses it first");
    let found: Vec<String> = compile::check(&root.expect("walks"), URL, &files)
        .iter()
        .map(ToString::to_string)
        .collect();
    assert!(
        has(
            &found,
            &[
                "field.md:6:",
                "`assets/huge.png` is 262145 bytes, over the 262144-byte cap"
            ]
        ),
        "{found:?}"
    );
}

/// A plain `http:` link reads as its text, and a secure one opens nothing
/// that can reach back into the app.
#[test]
fn a_link_is_secure_or_text() {
    let bundles = build(&with_deck(
        "::: card\n[plain](http://example.com) and [secure](https://example.com).\n:::\n",
    ));
    let (_, deck) = bundles[0].decks.iter().next().expect("one deck");
    let html = &deck.cards[0].html;
    assert!(!html.contains("http://"), "{html}");
    assert!(
        html.contains("<a href=\"https://example.com\" rel=\"noopener noreferrer\">secure</a>"),
        "{html}"
    );
}
