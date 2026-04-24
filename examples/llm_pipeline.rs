use budgetkernel::{Budget, ChargeError, Dim, Verdict};
use std::process::ExitCode;

fn checkpoint(budget: &mut Budget, tokens: u64, millis: u64) -> Result<Verdict, ChargeError> {
    let mut verdict = Verdict::Continue;

    verdict = verdict.worst(budget.charge(Dim::Tokens, tokens)?);
    verdict = verdict.worst(budget.charge(Dim::Millis, millis)?);
    verdict = verdict.worst(budget.charge(Dim::Calls, 1)?);

    Ok(verdict)
}

fn main() -> ExitCode {
    let mut budget = match Budget::builder()
        .limit_with_warn(Dim::Tokens, 10_000, 8_000)
        .limit_with_warn(Dim::Millis, 30_000, 27_000)
        .limit_with_warn(Dim::Calls, 5, 4)
        .build()
    {
        Ok(budget) => budget,
        Err(error) => {
            eprintln!("failed to build LLM budget: {error}");
            return ExitCode::FAILURE;
        }
    };

    let steps = [
        ("plan", 1_200, 2_500),
        ("retrieve", 2_400, 4_800),
        ("draft", 3_200, 8_000),
        ("critique", 1_400, 3_200),
        ("revise", 900, 2_400),
        ("extra-pass", 700, 1_600),
    ];

    for (name, tokens, millis) in steps {
        match checkpoint(&mut budget, tokens, millis) {
            Ok(Verdict::Continue) => {
                println!("{name}: continue");
            }
            Ok(Verdict::Warn(dim)) => {
                println!("{name}: warn on {}", dim.name());
            }
            Ok(Verdict::Exhausted(dim)) => {
                println!(
                    "{name}: exhausted on {}, stop and return partial result",
                    dim.name()
                );
                return ExitCode::SUCCESS;
            }
            Err(error) => {
                eprintln!("{name}: charge failed: {error}");
                return ExitCode::FAILURE;
            }
        }
    }

    println!(
        "done: tokens remaining = {:?}, millis remaining = {:?}, calls remaining = {:?}",
        budget.remaining(Dim::Tokens),
        budget.remaining(Dim::Millis),
        budget.remaining(Dim::Calls)
    );

    ExitCode::SUCCESS
}
