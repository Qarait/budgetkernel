use budgetkernel::{Budget, ChargeError, Dim, Verdict};
use std::process::ExitCode;

fn charge_request(budget: &mut Budget, response_bytes: u64) -> Result<Verdict, ChargeError> {
    let mut verdict = Verdict::Continue;

    verdict = verdict.worst(budget.charge(Dim::Calls, 1)?);
    verdict = verdict.worst(budget.charge(Dim::Bytes, response_bytes)?);

    Ok(verdict)
}

fn run_window(name: &str, budget: &mut Budget, responses: &[u64]) -> Result<(), ChargeError> {
    println!("{name}: starting window");

    for response_bytes in responses {
        match charge_request(budget, *response_bytes)? {
            Verdict::Continue => {
                println!("{name}: allow response of {response_bytes} bytes");
            }
            Verdict::Warn(dim) => {
                println!(
                    "{name}: allow response of {response_bytes} bytes, warn on {}",
                    dim.name()
                );
            }
            Verdict::Exhausted(dim) => {
                println!(
                    "{name}: reject after {} bytes, exhausted on {}",
                    response_bytes,
                    dim.name()
                );
                return Ok(());
            }
        }
    }

    Ok(())
}

fn main() -> ExitCode {
    let mut budget = match Budget::builder()
        .limit_with_warn(Dim::Calls, 3, 2)
        .limit_with_warn(Dim::Bytes, 10_000, 8_000)
        .build()
    {
        Ok(budget) => budget,
        Err(error) => {
            eprintln!("failed to build HTTP quota: {error}");
            return ExitCode::FAILURE;
        }
    };

    if let Err(error) = run_window("tenant-a/window-1", &mut budget, &[1_000, 2_500, 3_000]) {
        eprintln!("window failed: {error}");
        return ExitCode::FAILURE;
    }

    println!("tenant-a/window-1: manual reset");
    budget.reset();

    if let Err(error) = run_window("tenant-a/window-2", &mut budget, &[4_000, 4_500, 2_000]) {
        eprintln!("window failed: {error}");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}
