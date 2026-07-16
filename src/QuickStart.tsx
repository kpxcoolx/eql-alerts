import { useState } from "react";

type Step = {
  title: string;
  body: string;
};

const STEPS: Step[] = [
  {
    title: "Attach your character log",
    body: "EQL Alerts watches your EverQuest Legends log file. Click Find log (or Browse…) so it can see game chat and combat text in real time. In-game, make sure logging is on with /log on.",
  },
  {
    title: "Arm your class",
    body: "EQL Essentials Core / Combat / Danger / Fades stay on (noisy triggers like range/LOS stay off inside them). Arm Social if you want invites. Tap your class under Your class. Under EQL Raids, arm the zone bosses you run. Add your own alerts under Custom.",
  },
  {
    title: "Open the overlay",
    body: "Overlay shows toasts and countdown timers on top of the game. Place it where you like, then use Ctrl/Cmd+Alt+L for click-through so you can play through it, and Ctrl/Cmd+Alt+U to edit it again. Close hides it; open again from the main window.",
  },
];

type Props = {
  onSkip: () => void;
  onDone: () => void;
  onFindLog: () => void;
};

export default function QuickStart({ onSkip, onDone, onFindLog }: Props) {
  const [step, setStep] = useState(0);
  const current = STEPS[step];
  const last = step === STEPS.length - 1;

  return (
    <div className="qs-backdrop" role="dialog" aria-modal="true" aria-labelledby="qs-title">
      <div className="qs-card">
        <div className="qs-kicker">Quick start</div>
        <h1 id="qs-title">Get alerts running in a minute</h1>
        <p className="qs-lead">
          Watches your Legends log and fires text / timer alerts — like GINA, without the
          homework.
        </p>

        <div className="qs-steps" aria-hidden="true">
          {STEPS.map((_, i) => (
            <span
              key={i}
              className={`qs-dot ${i === step ? "on" : ""} ${i < step ? "done" : ""}`}
            />
          ))}
        </div>

        <div className="qs-panel">
          <div className="qs-step-num">
            Step {step + 1} of {STEPS.length}
          </div>
          <h2>{current.title}</h2>
          <p>{current.body}</p>
        </div>

        <div className="qs-actions">
          <button className="btn ghost" type="button" onClick={onSkip}>
            Skip — I know what I’m doing
          </button>
          <div className="qs-actions-right">
            {step > 0 ? (
              <button className="btn" type="button" onClick={() => setStep((s) => s - 1)}>
                Back
              </button>
            ) : null}
            {step === 0 ? (
              <button
                className="btn primary"
                type="button"
                onClick={() => {
                  onFindLog();
                  setStep(1);
                }}
              >
                Find log & continue
              </button>
            ) : last ? (
              <button className="btn primary" type="button" onClick={onDone}>
                Start playing
              </button>
            ) : (
              <button className="btn primary" type="button" onClick={() => setStep((s) => s + 1)}>
                Next
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
