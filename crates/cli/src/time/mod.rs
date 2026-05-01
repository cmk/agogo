//! Musical-time CLI commands.

pub mod schedule;

use bpaf::Bpaf;

#[derive(Debug, Clone, Bpaf)]
pub enum TimeOp {
    /// Print absolute tick positions for a schedule at a given TBase.
    /// On off-beat 16th-note steps the swing shift (if any) is
    /// applied before printing, so positive swing delays odd steps
    /// relative to their nominal grid position.
    #[bpaf(command("schedule"))]
    Schedule(#[bpaf(external(schedule::schedule_args))] schedule::ScheduleArgs),
}

pub fn dispatch(op: TimeOp) -> Result<(), String> {
    match op {
        TimeOp::Schedule(args) => {
            // Header to stderr (stdout reserved for the schedule
            // itself). Emits the parsed inputs so the reader can
            // correlate against the tick stream.
            eprintln!(
                "# schedule: {} bars, grid={}, swing={:.3}",
                args.bars, args.grid, args.swing
            );
            for t in schedule::schedule_ticks(&args) {
                println!("{}", t.0);
            }
            Ok(())
        }
    }
}
