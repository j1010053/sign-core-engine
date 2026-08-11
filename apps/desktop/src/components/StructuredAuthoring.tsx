import { useMutation, useQuery } from "@tanstack/react-query";
import { CheckCircle2, RotateCcw } from "lucide-react";
import { useEffect, useState } from "react";
import type { ChangeEvent, ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type {
  AuthoringCatalog,
  AuthoringField,
  BodyItemInput,
  PendingChange,
  StructuredEdit,
} from "../contracts";
import { api } from "../ipc";
import { ErrorNotice } from "./ErrorNotice";

type Tool =
  | "sound_change"
  | "insert_sign"
  | "insert_trait"
  | "clone_sign"
  | "insert_body"
  | "delete"
  | "update"
  | "move";

type Submission =
  | { kind: "sound_change"; rule: string; home: string }
  | { kind: "structured"; edit: StructuredEdit };

type ToolProps = {
  catalog: AuthoringCatalog;
  disabled: boolean;
  submit: (submission: Submission) => void;
};

type StructuredAuthoringProps = {
  catalog?: AuthoringCatalog;
  rawDirty: boolean;
  statements: number;
  onStaged: (pending: PendingChange) => void | Promise<void>;
  onDiscard: () => void;
  discarding: boolean;
};

const tools: Tool[] = [
  "sound_change",
  "insert_sign",
  "insert_trait",
  "clone_sign",
  "insert_body",
  "delete",
  "update",
  "move",
];

function selectedValues(event: ChangeEvent<HTMLSelectElement>): string[] {
  return Array.from(event.currentTarget.selectedOptions, (option) => option.value);
}

function traitChoices(catalog: AuthoringCatalog) {
  return catalog.traits.filter((trait) => !trait.global);
}

function SoundChangeForm({ catalog, disabled, submit }: ToolProps) {
  const { t } = useTranslation();
  const [rule, setRule] = useState("");
  const [home, setHome] = useState("");
  return (
    <div className="form-stack">
      <label>
        {t("editor.rule")}
        <textarea
          rows={4}
          value={rule}
          placeholder="t => k"
          onChange={(event) => setRule(event.target.value)}
        />
      </label>
      <label>
        {t("editor.home")}
        <select value={home} onChange={(event) => setHome(event.target.value)}>
          <option value="">{t("editor.structured.chooseHome")}</option>
          {catalog.rule_homes.map((choice) => (
            <option key={choice.value} value={choice.value}>{choice.label}</option>
          ))}
        </select>
      </label>
      {catalog.rule_homes.length === 0 && (
        <p className="authoring-hint">{t("editor.structured.noRuleHomes")}</p>
      )}
      <button
        className="button secondary"
        type="button"
        disabled={disabled || !rule.trim() || !home}
        onClick={() => submit({ kind: "sound_change", rule, home })}
      >
        {t("editor.stage")}
      </button>
    </div>
  );
}

function InsertSignForm({ catalog, disabled, submit }: ToolProps) {
  const { t } = useTranslation();
  const [name, setName] = useState("");
  const [belongs, setBelongs] = useState<string[]>([]);
  const [phon, setPhon] = useState("");
  const [gloss, setGloss] = useState("");
  return (
    <div className="form-stack">
      <label>{t("editor.structured.name")}<input value={name} onChange={(event) => setName(event.target.value)} /></label>
      <label>
        {t("editor.structured.belongs")}
        <select multiple size={Math.min(6, Math.max(3, traitChoices(catalog).length))} value={belongs} onChange={(event) => setBelongs(selectedValues(event))}>
          {traitChoices(catalog).map((trait) => (
            <option key={trait.name} value={trait.name}>{trait.name}{trait.source === "library" ? " · library" : ""}</option>
          ))}
        </select>
      </label>
      <div className="field-grid">
        <label>{t("editor.structured.phon")}<input value={phon} onChange={(event) => setPhon(event.target.value)} /></label>
        <label>{t("editor.structured.gloss")}<input value={gloss} onChange={(event) => setGloss(event.target.value)} /></label>
      </div>
      <button className="button secondary" type="button" disabled={disabled || !name.trim()} onClick={() => submit({ kind: "structured", edit: { action: "insert_sign", name, belongs, ...(phon.trim() ? { phon } : {}), ...(gloss.trim() ? { gloss } : {}) } })}>{t("editor.stage")}</button>
    </div>
  );
}

function InsertTraitForm({ catalog, disabled, submit }: ToolProps) {
  const { t } = useTranslation();
  const [name, setName] = useState("");
  const [global, setGlobal] = useState(false);
  const [parent, setParent] = useState("");
  return (
    <div className="form-stack">
      <label>{t("editor.structured.name")}<input value={name} onChange={(event) => setName(event.target.value)} /></label>
      <label className="checkbox-field"><input type="checkbox" checked={global} onChange={(event) => setGlobal(event.target.checked)} />{t("editor.structured.global")}</label>
      <label>
        {t("editor.structured.parent")}
        <select value={parent} onChange={(event) => setParent(event.target.value)}>
          <option value="">{t("common.none")}</option>
          {traitChoices(catalog).map((trait) => <option key={trait.name} value={trait.name}>{trait.name}</option>)}
        </select>
      </label>
      <button className="button secondary" type="button" disabled={disabled || !name.trim()} onClick={() => submit({ kind: "structured", edit: { action: "insert_trait", name, global, ...(parent ? { parent } : {}) } })}>{t("editor.stage")}</button>
    </div>
  );
}

function CloneSignForm({ catalog, disabled, submit }: ToolProps) {
  const { t } = useTranslation();
  const [source, setSource] = useState("");
  const [name, setName] = useState("");
  return (
    <div className="form-stack">
      <label>
        {t("editor.structured.sourceSign")}
        <select value={source} onChange={(event) => setSource(event.target.value)}>
          <option value="">{t("editor.structured.chooseNode")}</option>
          {catalog.signs.map((sign) => <option key={sign.selector} value={sign.selector}>{sign.name}</option>)}
        </select>
      </label>
      <label>{t("editor.structured.newName")}<input value={name} onChange={(event) => setName(event.target.value)} /></label>
      <button className="button secondary" type="button" disabled={disabled || !source || !name.trim()} onClick={() => submit({ kind: "structured", edit: { action: "clone_sign", source, name } })}>{t("editor.stage")}</button>
    </div>
  );
}

function InsertBodyForm({ catalog, disabled, submit }: ToolProps) {
  const { t } = useTranslation();
  const [container, setContainer] = useState("");
  const [kind, setKind] = useState<BodyItemInput["kind"]>("belongs");
  const [traitName, setTraitName] = useState("");
  const [name, setName] = useState("");
  const [constraint, setConstraint] = useState("*");
  const [optional, setOptional] = useState(false);
  const [dim, setDim] = useState<"phon" | "syn" | "sem" | "prag">("syn");
  const [enumValues, setEnumValues] = useState("");
  const [value, setValue] = useState("");
  const [path, setPath] = useState("");
  const [body, setBody] = useState("");
  const [stage, setStage] = useState<"stem" | "word" | "phrase">("word");

  const makeBody = (): BodyItemInput => {
    switch (kind) {
      case "belongs": return { kind, trait_name: traitName };
      case "trait_use": return { kind, trait_name: traitName };
      case "slot": return { kind, name, constraint, optional };
      case "feature": return { kind, dim: dim === "phon" ? "syn" : dim, name, enum_values: enumValues.split(",").map((item) => item.trim()).filter(Boolean), value };
      case "sense": return { kind, name, gloss: value };
      case "phon": return { kind, form: value };
      case "definition": return { kind, dim: dim === "phon" ? "syn" : dim, path, value };
      case "rule": return { kind, dim, body, ...(name.trim() ? { name } : {}), stage };
    }
  };
  const ready = container && (
    (kind === "belongs" || kind === "trait_use") ? traitName :
      kind === "slot" ? name && constraint :
        kind === "feature" ? name && (enumValues.trim() || value.trim()) :
          kind === "sense" ? name && value.trim() :
            kind === "phon" ? value.trim() :
              kind === "definition" ? path.trim() && value.trim() : body.trim()
  );
  const traitSelect = (
    <label>
      {t("editor.structured.trait")}
      <select value={traitName} onChange={(event) => setTraitName(event.target.value)}>
        <option value="">{t("editor.structured.chooseTrait")}</option>
        {traitChoices(catalog).map((trait) => <option key={trait.name} value={trait.name}>{trait.name}{trait.blocks > 1 ? ` · ${trait.blocks} blocks` : ""}</option>)}
      </select>
    </label>
  );
  return (
    <div className="form-stack">
      <label>
        {t("editor.structured.container")}
        <select value={container} onChange={(event) => setContainer(event.target.value)}>
          <option value="">{t("editor.structured.chooseNode")}</option>
          {catalog.body_containers.map((choice) => <option key={choice.value} value={choice.value}>{choice.label}</option>)}
        </select>
      </label>
      <label>
        {t("editor.structured.bodyKind")}
        <select value={kind} onChange={(event) => setKind(event.target.value as BodyItemInput["kind"])}>
          {(["belongs", "trait_use", "slot", "feature", "sense", "phon", "definition", "rule"] as const).map((item) => <option key={item} value={item}>{t(`editor.structured.bodyKinds.${item}`)}</option>)}
        </select>
      </label>
      {(kind === "belongs" || kind === "trait_use") && traitSelect}
      {kind === "slot" && <><div className="field-grid"><label>{t("editor.structured.slotName")}<input value={name} onChange={(event) => setName(event.target.value)} /></label><label>{t("editor.structured.constraint")}<select value={constraint} onChange={(event) => setConstraint(event.target.value)}><option value="*">*</option>{traitChoices(catalog).map((trait) => <option key={trait.name} value={trait.name}>{trait.name}</option>)}</select></label></div><label className="checkbox-field"><input type="checkbox" checked={optional} onChange={(event) => setOptional(event.target.checked)} />{t("editor.structured.optional")}</label></>}
      {kind === "feature" && <><div className="field-grid"><DimensionSelect value={dim === "phon" ? "syn" : dim} feature onChange={setDim} /><label>{t("editor.structured.featureName")}<input value={name} onChange={(event) => setName(event.target.value)} /></label></div><label>{t("editor.structured.enumValues")}<input value={enumValues} placeholder="singular, plural" onChange={(event) => setEnumValues(event.target.value)} /></label><label>{t("editor.structured.value")}<input value={value} onChange={(event) => setValue(event.target.value)} /></label></>}
      {kind === "sense" && <><label>{t("editor.structured.senseName")}<input value={name} onChange={(event) => setName(event.target.value)} /></label><label>{t("editor.structured.gloss")}<input value={value} onChange={(event) => setValue(event.target.value)} /></label></>}
      {kind === "phon" && <label>{t("editor.structured.phon")}<input value={value} onChange={(event) => setValue(event.target.value)} /></label>}
      {kind === "definition" && <><DimensionSelect value={dim === "phon" ? "syn" : dim} feature onChange={setDim} /><label>{t("editor.structured.path")}<input value={path} onChange={(event) => setPath(event.target.value)} /></label><label>{t("editor.structured.value")}<input value={value} onChange={(event) => setValue(event.target.value)} /></label></>}
      {kind === "rule" && <><DimensionSelect value={dim} onChange={setDim} /><label>{t("editor.structured.ruleBody")}<textarea rows={3} value={body} onChange={(event) => setBody(event.target.value)} /></label><div className="field-grid"><label>{t("editor.structured.ruleName")}<input value={name} onChange={(event) => setName(event.target.value)} /></label><label>{t("editor.structured.stage")}<select value={stage} onChange={(event) => setStage(event.target.value as typeof stage)}><option value="stem">stem</option><option value="word">word</option><option value="phrase">phrase</option></select></label></div></>}
      <button className="button secondary" type="button" disabled={disabled || !ready} onClick={() => submit({ kind: "structured", edit: { action: "insert_body", container, body: makeBody() } })}>{t("editor.stage")}</button>
    </div>
  );
}

function DimensionSelect({ value, onChange, feature = false }: { value: "phon" | "syn" | "sem" | "prag"; onChange: (value: "phon" | "syn" | "sem" | "prag") => void; feature?: boolean }) {
  const { t } = useTranslation();
  return <label>{t("editor.structured.dimension")}<select value={value} onChange={(event) => onChange(event.target.value as typeof value)}>{(!feature ? ["phon", "syn", "sem", "prag"] : ["syn", "sem", "prag"]).map((item) => <option key={item} value={item}>{item}</option>)}</select></label>;
}

function NodeSelect({ label, value, onChange, nodes }: { label: string; value: string; onChange: (value: string) => void; nodes: AuthoringCatalog["nodes"] }) {
  const { t } = useTranslation();
  return <label>{label}<select value={value} onChange={(event) => onChange(event.target.value)}><option value="">{t("editor.structured.chooseNode")}</option>{nodes.map((node) => <option key={node.selector} value={node.selector}>{node.path} · {node.kind}</option>)}</select></label>;
}

function DeleteForm({ catalog, disabled, submit }: ToolProps) {
  const { t } = useTranslation();
  const [target, setTarget] = useState("");
  const remove = () => {
    if (window.confirm(t("editor.structured.deleteConfirm"))) submit({ kind: "structured", edit: { action: "delete", target } });
  };
  return <div className="form-stack"><NodeSelect label={t("editor.structured.target")} value={target} onChange={setTarget} nodes={catalog.nodes.filter((node) => node.deletable)} /><p className="authoring-hint">{t("editor.structured.deleteHint")}</p><button className="button danger" type="button" disabled={disabled || !target} onClick={remove}>{t("common.delete")}</button></div>;
}

function FieldControl({ field, value, onChange }: { field?: AuthoringField; value: string; onChange: (value: string) => void }) {
  if (!field) return null;
  if (field.control === "textarea") return <label>{field.label}<textarea rows={4} value={value} onChange={(event) => onChange(event.target.value)} /></label>;
  if (field.control === "boolean" || field.control === "choice") return <label>{field.label}<select value={value} onChange={(event) => onChange(event.target.value)}><option value="" />{field.choices.map((choice) => <option key={choice.value} value={choice.value}>{choice.label}</option>)}</select></label>;
  return <label>{field.label}<input value={value} onChange={(event) => onChange(event.target.value)} /></label>;
}

function UpdateForm({ catalog, disabled, submit }: ToolProps) {
  const { t } = useTranslation();
  const [target, setTarget] = useState("");
  const [fieldName, setFieldName] = useState("");
  const [value, setValue] = useState("");
  const node = catalog.nodes.find((candidate) => candidate.selector === target);
  const selectedField = node?.fields.find((field) => field.name === fieldName);
  useEffect(() => {
    if (!node?.fields.some((field) => field.name === fieldName)) {
      setFieldName(node?.fields[0]?.name ?? "");
      setValue("");
    }
  }, [fieldName, node]);
  return <div className="form-stack"><NodeSelect label={t("editor.structured.target")} value={target} onChange={(next) => { setTarget(next); setValue(""); }} nodes={catalog.nodes.filter((candidate) => candidate.fields.length > 0)} /><label>{t("editor.structured.field")}<select value={fieldName} onChange={(event) => { setFieldName(event.target.value); setValue(""); }}><option value="" />{node?.fields.map((field) => <option key={field.name} value={field.name}>{field.label}</option>)}</select></label><FieldControl field={selectedField} value={value} onChange={setValue} /><button className="button secondary" type="button" disabled={disabled || !target || !fieldName || (selectedField?.control !== "text" && selectedField?.control !== "textarea" && !value)} onClick={() => submit({ kind: "structured", edit: { action: "update", target, field: fieldName, value } })}>{t("editor.stage")}</button></div>;
}

function MoveForm({ catalog, disabled, submit }: ToolProps) {
  const { t } = useTranslation();
  const [target, setTarget] = useState("");
  const [placementIndex, setPlacementIndex] = useState("");
  const options = useQuery({
    queryKey: ["authoring-move", catalog.revision, target],
    queryFn: () => api.authoringMoveOptions(target, catalog.revision),
    enabled: Boolean(target) && !disabled,
    retry: false,
  });
  useEffect(() => setPlacementIndex(""), [target, catalog.revision]);
  const placement = placementIndex ? options.data?.placements[Number(placementIndex)] : undefined;
  return <div className="form-stack"><NodeSelect label={t("editor.structured.target")} value={target} onChange={setTarget} nodes={catalog.nodes.filter((node) => node.movable)} /><label>{t("editor.structured.placement")}<select value={placementIndex} disabled={!target || options.isLoading || disabled} onChange={(event) => setPlacementIndex(event.target.value)}><option value="">{options.isLoading ? t("common.loading") : t("editor.structured.choosePlacement")}</option>{options.data?.placements.map((item, index) => <option key={`${item.parent}:${item.position}:${item.sibling ?? ""}`} value={String(index)}>{item.label}</option>)}</select></label>{options.error && <ErrorNotice error={options.error} />}<button className="button secondary" type="button" disabled={disabled || !placement} onClick={() => placement && submit({ kind: "structured", edit: { action: "move", target, placement: { parent: placement.parent, position: placement.position, ...(placement.sibling ? { sibling: placement.sibling } : {}) } } })}>{t("editor.stage")}</button></div>;
}

export function StructuredAuthoring({ catalog, rawDirty, statements, onStaged, onDiscard, discarding }: StructuredAuthoringProps) {
  const { t } = useTranslation();
  const [tool, setTool] = useState<Tool>("sound_change");
  const [resetSerial, setResetSerial] = useState(0);
  const [feedback, setFeedback] = useState("");
  const stage = useMutation({
    mutationFn: async (submission: Submission) => {
      if (!catalog) throw new Error(t("editor.structured.catalogUnavailable"));
      return submission.kind === "sound_change"
        ? api.stageSoundChange(submission.rule, submission.home, catalog.revision)
        : api.stageStructuredEdit({ revision: catalog.revision, ...submission.edit });
    },
    onSuccess: async (pending) => {
      setResetSerial((serial) => serial + 1);
      setFeedback(t("editor.structured.staged", { count: pending.statements }));
      await onStaged(pending);
    },
  });
  const disabled = rawDirty || !catalog || stage.isPending;
  const props = catalog ? { catalog, disabled, submit: stage.mutate } : undefined;
  const form = props && ({
    sound_change: <SoundChangeForm {...props} />,
    insert_sign: <InsertSignForm {...props} />,
    insert_trait: <InsertTraitForm {...props} />,
    clone_sign: <CloneSignForm {...props} />,
    insert_body: <InsertBodyForm {...props} />,
    delete: <DeleteForm {...props} />,
    update: <UpdateForm {...props} />,
    move: <MoveForm {...props} />,
  } satisfies Record<Tool, ReactNode>)[tool];

  return (
    <section className="panel structured-authoring" aria-labelledby="structured-authoring-title">
      <div className="section-heading compact"><div><p className="eyebrow">STRUCTURED</p><h2 id="structured-authoring-title">{t("editor.structured.title")}</h2></div></div>
      <div className="authoring-tools" role="tablist" aria-label={t("editor.structured.tools")}>
        {tools.map((item) => <button key={item} type="button" role="tab" aria-selected={tool === item} className={tool === item ? "active" : ""} onClick={() => { setTool(item); setFeedback(""); }}>{t(`editor.structured.tool.${item}`)}</button>)}
      </div>
      {rawDirty && <div className="status-banner warning">{t("editor.structured.rawDirty")}</div>}
      {!catalog && <p className="authoring-hint">{t("common.loading")}</p>}
      <div key={`${tool}:${resetSerial}`} className="authoring-form" role="tabpanel">{form}</div>
      {stage.error && <ErrorNotice error={stage.error} />}
      {feedback && <div className="status-banner success"><CheckCircle2 />{feedback}</div>}
      <button className="button ghost" type="button" disabled={rawDirty || !statements || discarding} onClick={onDiscard}><RotateCcw />{t("editor.undoStatement")}</button>
    </section>
  );
}
