import type { DiffSummary as Diff } from "../contracts";

export function DiffSummary({ diff }: { diff: Diff }) {
  const values = [
    ["born", diff.born], ["died", diff.died], ["phon", diff.phon], ["syn", diff.syn],
    ["sem", diff.sem], ["prag", diff.prag], ["struct", diff.structural],
  ];
  return <div className="diff-strip">{values.map(([key, value]) => <div key={key}><span>{key}</span><strong>{value}</strong></div>)}</div>;
}

