use budgetkernel::{Budget, ChargeError, Dim, Verdict};
use std::process::ExitCode;

#[derive(Copy, Clone)]
struct Task {
    name: &'static str,
    millis: u64,
    bytes: u64,
    work_units: u64,
}

fn charge_task(budget: &mut Budget, task: Task) -> Result<Verdict, ChargeError> {
    let mut verdict = Verdict::Continue;

    verdict = verdict.worst(budget.charge(Dim::Calls, 1)?);
    verdict = verdict.worst(budget.charge(Dim::Millis, task.millis)?);
    verdict = verdict.worst(budget.charge(Dim::Bytes, task.bytes)?);
    verdict = verdict.worst(budget.charge(Dim::Custom0, task.work_units)?);

    Ok(verdict)
}

fn main() -> ExitCode {
    let mut budget = match Budget::builder()
        .limit_with_warn(Dim::Calls, 4, 3)
        .limit_with_warn(Dim::Millis, 12_000, 9_000)
        .limit_with_warn(Dim::Bytes, 2_000_000, 1_600_000)
        .limit_with_warn(Dim::Custom0, 100, 80)
        .build()
    {
        Ok(budget) => budget,
        Err(error) => {
            eprintln!("failed to build task budget: {error}");
            return ExitCode::FAILURE;
        }
    };

    let tasks = [
        Task {
            name: "parse",
            millis: 1_500,
            bytes: 200_000,
            work_units: 10,
        },
        Task {
            name: "index",
            millis: 3_000,
            bytes: 650_000,
            work_units: 25,
        },
        Task {
            name: "rank",
            millis: 4_000,
            bytes: 500_000,
            work_units: 35,
        },
        Task {
            name: "summarize",
            millis: 3_200,
            bytes: 450_000,
            work_units: 20,
        },
        Task {
            name: "extra-cleanup",
            millis: 1_200,
            bytes: 120_000,
            work_units: 8,
        },
    ];

    for task in tasks {
        match charge_task(&mut budget, task) {
            Ok(Verdict::Continue) => {
                println!("{}: continue", task.name);
            }
            Ok(Verdict::Warn(dim)) => {
                println!(
                    "{}: warn on {}, reduce work or degrade quality",
                    task.name,
                    dim.name()
                );
            }
            Ok(Verdict::Exhausted(dim)) => {
                println!(
                    "{}: exhausted on {}, stop task runner",
                    task.name,
                    dim.name()
                );
                return ExitCode::SUCCESS;
            }
            Err(error) => {
                eprintln!("{}: charge failed: {error}", task.name);
                return ExitCode::FAILURE;
            }
        }
    }

    println!(
        "done: calls={:?}, millis={:?}, bytes={:?}, work_units={:?}",
        budget.spent(Dim::Calls),
        budget.spent(Dim::Millis),
        budget.spent(Dim::Bytes),
        budget.spent(Dim::Custom0)
    );

    ExitCode::SUCCESS
}
