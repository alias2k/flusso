// The function behind `rules` / `computed` must give the field's own type.
use flusso_query::FlussoRoot;
use flusso_query::rules::Rules;

#[derive(Clone)]
struct Viewing;

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "contracts", context = Viewing)]
struct Contract {
    id: String,
    #[flusso(rules = phase)]
    phase: bool,
    #[flusso(computed = days_left)]
    days_left: i64,
}

fn phase(_: &Viewing) -> Rules<String> {
    Rules::when(Contract::doc().signed_at().missing())
        .then("draft".to_owned())
        .otherwise("signed".to_owned())
}

#[derive(serde::Deserialize)]
struct Term {
    notice_days: i32,
}

fn days_left(term: &Term, _: &Viewing) -> String {
    term.notice_days.to_string()
}

fn main() {}
