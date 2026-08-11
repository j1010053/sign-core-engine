import { z } from "zod";

export const UI_SCHEMA = "conlang.ui/v1" as const;

export const uiErrorSchema = z
  .object({ code: z.string(), message: z.string() })
  .strict();
export type UiError = z.infer<typeof uiErrorSchema>;

const contactSchema = z
  .object({
    counterpart: z.string(),
    period: z.string().optional(),
    intensity: z.enum(["sporadic", "trade", "bilingual", "dominant"]),
  })
  .strict();

export const evolutionStateSchema = z
  .object({
    time: z.string().optional(),
    region: z.string().optional(),
    society: z.array(z.string()).default([]),
    contacts: z.array(contactSchema).default([]),
  })
  .strict();
export type EvolutionState = z.infer<typeof evolutionStateSchema>;

const treeEdgeSchema = z
  .object({ from: z.string(), kind: z.enum(["trunk", "reference"]) })
  .strict();
const treeNodeSchema = z
  .object({
    id: z.string(),
    label: z.string().optional(),
    parents: z.array(treeEdgeSchema).default([]),
  })
  .strict();
export const evolutionTreeSchema = z
  .object({
    schema: z.literal(UI_SCHEMA),
    nodes: z.array(treeNodeSchema),
    active: z.string().optional(),
  })
  .strict();
export type EvolutionTree = z.infer<typeof evolutionTreeSchema>;
export type EvolutionTreeNode = z.infer<typeof treeNodeSchema>;

const dimSchema = z.enum(["phon", "syn", "sem", "prag"]);
const lexiconEntrySchema = z
  .object({
    name: z.string(),
    categories: z.array(z.string()),
    underlying_form: z.string().nullable().optional(),
    gloss: z.string().nullable().optional(),
    senses: z.array(z.tuple([z.string(), z.string()])),
    dimensions: z.array(z.tuple([dimSchema, z.array(z.tuple([z.string(), z.string()]))])),
  })
  .strict();
export const lexiconViewSchema = z
  .object({
    schema: z.literal(UI_SCHEMA),
    node: z.string(),
    lexicon: z
      .object({
        entries: z.array(lexiconEntrySchema),
        total_before_filter: z.number().int().nonnegative(),
      })
      .strict(),
  })
  .strict();
export type LexiconView = z.infer<typeof lexiconViewSchema>;
export type LexiconEntry = z.infer<typeof lexiconEntrySchema>;

export const nodeDetailSchema = z
  .object({
    schema: z.literal(UI_SCHEMA),
    id: z.string(),
    label: z.string().optional(),
    state: evolutionStateSchema,
    annotations: z.array(z.string()).default([]),
    sign_count: z.number().int().nonnegative(),
  })
  .strict();
export type NodeDetail = z.infer<typeof nodeDetailSchema>;

export const projectSummarySchema = z
  .object({
    schema: z.literal(UI_SCHEMA),
    path: z.string(),
    name: z.string().optional(),
    legacy: z.boolean(),
    graph_dirty: z.boolean(),
    has_pending: z.boolean(),
    node_count: z.number().int().nonnegative(),
    active: z.string().optional(),
    packages: z.array(z.string()),
  })
  .strict();
export type ProjectSummary = z.infer<typeof projectSummarySchema>;

const catalogPackageSchema = z
  .object({
    id: z.string(),
    // Package-loader v2 exposes the open namespace here. Legacy values remain
    // ordinary namespaces rather than a closed discriminator.
    kind: z.string().min(1),
    version: z.string(),
    source: z.enum(["embedded", "vendored", "installed", "injected"]),
    enabled: z.boolean(),
    declared: z.boolean(),
    selected: z.boolean(),
    requires: z.array(z.string()).default([]),
  })
  .strict();
export const packageCatalogSchema = z
  .object({ schema: z.literal(UI_SCHEMA), packages: z.array(catalogPackageSchema) })
  .strict();
export type CatalogPackage = z.infer<typeof catalogPackageSchema>;
export type PackageCatalog = z.infer<typeof packageCatalogSchema>;
export type PackageSelection =
  | { roots: string[]; aliases?: Record<string, string> }
  | { std?: string[]; natural?: string; plugins?: string[] };

const weightEntrySchema = z
  .object({
    segment: z.string(),
    weight: z.number().nonnegative(),
    source: z.enum(["manual", "imported", "prior"]),
  })
  .strict();
export const weightConfigSchema = z
  .object({
    schema: z.literal(UI_SCHEMA),
    declaration_source: z.literal("project.toml:weights"),
    manual: z.array(weightEntrySchema),
    effective: z.array(weightEntrySchema),
  })
  .strict();
export type SegmentWeight = { segment: string; weight: number };
export type WeightConfig = z.infer<typeof weightConfigSchema>;

export const diffSummarySchema = z
  .object({
    aligned: z.number().int().nonnegative(),
    born: z.number().int().nonnegative(),
    died: z.number().int().nonnegative(),
    phon: z.number().int().nonnegative(),
    syn: z.number().int().nonnegative(),
    sem: z.number().int().nonnegative(),
    prag: z.number().int().nonnegative(),
    structural: z.number().int().nonnegative(),
  })
  .strict();
export type DiffSummary = z.infer<typeof diffSummarySchema>;

export const pendingChangeSchema = z
  .object({
    schema: z.literal(UI_SCHEMA),
    source: z.string(),
    statements: z.number().int().nonnegative(),
    diff: diffSummarySchema,
  })
  .strict();
export type PendingChange = z.infer<typeof pendingChangeSchema>;

const authoringChoiceSchema = z
  .object({ value: z.string(), label: z.string() })
  .strict();
const authoringFieldSchema = z
  .object({
    name: z.string(),
    label: z.string(),
    control: z.enum(["text", "textarea", "boolean", "choice"]),
    choices: z.array(authoringChoiceSchema).default([]),
  })
  .strict();
const authoringNodeSchema = z
  .object({
    selector: z.string(),
    parent: z.string().optional(),
    kind: z.string(),
    path: z.string(),
    summary: z.string(),
    deletable: z.boolean(),
    movable: z.boolean(),
    fields: z.array(authoringFieldSchema).default([]),
  })
  .strict();
const authoringSignSchema = z
  .object({ name: z.string(), selector: z.string() })
  .strict();
const authoringTraitSchema = z
  .object({
    name: z.string(),
    global: z.boolean(),
    blocks: z.number().int().nonnegative(),
    source: z.enum(["local", "library"]),
    selector: z.string().optional(),
  })
  .strict();
export const authoringCatalogSchema = z
  .object({
    schema: z.literal(UI_SCHEMA),
    revision: z.string(),
    nodes: z.array(authoringNodeSchema),
    signs: z.array(authoringSignSchema),
    traits: z.array(authoringTraitSchema),
    rule_homes: z.array(authoringChoiceSchema),
    body_containers: z.array(authoringChoiceSchema),
  })
  .strict();
export type AuthoringCatalog = z.infer<typeof authoringCatalogSchema>;
export type AuthoringField = z.infer<typeof authoringFieldSchema>;

const authoringMoveOptionSchema = z
  .object({
    parent: z.string(),
    parent_label: z.string(),
    position: z.enum(["start", "end", "before", "after"]),
    sibling: z.string().optional(),
    label: z.string(),
  })
  .strict();
export const authoringMoveOptionsSchema = z
  .object({
    schema: z.literal(UI_SCHEMA),
    revision: z.string(),
    target: z.string(),
    placements: z.array(authoringMoveOptionSchema),
  })
  .strict();
export type AuthoringMoveOptions = z.infer<typeof authoringMoveOptionsSchema>;
export type MovePlacementInput = Pick<
  z.infer<typeof authoringMoveOptionSchema>,
  "parent" | "position" | "sibling"
>;

export type BodyItemInput =
  | { kind: "belongs"; trait_name: string }
  | { kind: "trait_use"; trait_name: string }
  | { kind: "slot"; name: string; constraint: string; optional: boolean }
  | {
      kind: "feature";
      dim: "syn" | "sem" | "prag";
      name: string;
      enum_values: string[];
      value: string;
    }
  | { kind: "sense"; name: string; gloss: string }
  | { kind: "phon"; form: string }
  | {
      kind: "definition";
      dim: "syn" | "sem" | "prag";
      path: string;
      value: string;
    }
  | {
      kind: "rule";
      dim: "phon" | "syn" | "sem" | "prag";
      body: string;
      name?: string;
      stage: "stem" | "word" | "phrase";
    };

export type StructuredEdit =
  | { action: "insert_sign"; name: string; belongs: string[]; phon?: string; gloss?: string }
  | { action: "insert_trait"; name: string; global: boolean; parent?: string }
  | { action: "clone_sign"; source: string; name: string }
  | { action: "insert_body"; container: string; body: BodyItemInput }
  | { action: "delete"; target: string }
  | { action: "update"; target: string; field: string; value: string }
  | { action: "move"; target: string; placement: MovePlacementInput };

export type StructuredEditInput = { revision: string } & StructuredEdit;

export const sourceViewSchema = z
  .object({ schema: z.literal(UI_SCHEMA), node: z.string(), source: z.string() })
  .strict();
export type SourceView = z.infer<typeof sourceViewSchema>;

export const sourceReconcileSchema = z
  .object({
    schema: z.literal(UI_SCHEMA),
    matched: z.number().int().nonnegative(),
    inserted: z.number().int().nonnegative(),
    deleted: z.number().int().nonnegative(),
    primitive_edits: z.number().int().nonnegative(),
    pending: pendingChangeSchema,
  })
  .strict();
export type SourceReconcile = z.infer<typeof sourceReconcileSchema>;

export const rebasePreviewSchema = z
  .object({
    schema: z.literal(UI_SCHEMA),
    node: z.string(),
    onto: z.string(),
    status: z.enum(["clean", "conflict", "environment", "broken"]),
    statement: z.number().int().nonnegative().optional(),
    message: z.string().optional(),
    result: z.string().optional(),
  })
  .strict();
export type RebasePreview = z.infer<typeof rebasePreviewSchema>;

export const proposalSchema = z
  .object({ phon: z.string(), score: z.number(), rationale: z.string() })
  .strict();
export const proposalsViewSchema = z
  .object({
    schema: z.literal(UI_SCHEMA),
    node: z.string(),
    proposals: z.array(proposalSchema),
  })
  .strict();
export type ProposalsView = z.infer<typeof proposalsViewSchema>;

export const statsViewSchema = z
  .object({
    schema: z.literal(UI_SCHEMA),
    node: z.string(),
    segmentation: z.string(),
    sampling_source: z.literal(false),
    segments: z.array(
      z.object({ segment: z.string(), count: z.number().nonnegative() }).strict(),
    ),
  })
  .strict();
export type StatsView = z.infer<typeof statsViewSchema>;

export const groupingViewSchema = z
  .object({
    schema: z.literal(UI_SCHEMA),
    grouping: z
      .object({
        members: z.record(z.string(), z.string()),
        labels: z.record(z.string(), z.string()),
        measure_id: z.string(),
        threshold: z.number(),
      })
      .strict(),
  })
  .strict();
export type GroupingView = z.infer<typeof groupingViewSchema>;

const intelligibilityScoreSchema = z
  .object({ value: z.number(), measure_id: z.string(), symmetric: z.boolean() })
  .strict();
export const intelligibilityViewSchema = z
  .object({
    schema: z.literal(UI_SCHEMA),
    source: z.string(),
    target: z.string(),
    score: intelligibilityScoreSchema,
    diff: diffSummarySchema,
  })
  .strict();
export type IntelligibilityView = z.infer<typeof intelligibilityViewSchema>;

const senseLinkSchema = z
  .object({
    to: z.string(),
    from: z.string(),
    kind: z.string(),
    transparency: z.string(),
  })
  .strict();
const derivationNodeSchema = z
  .object({
    name: z.string(),
    origin: z.string().nullable().optional(),
    underlying_form: z.string().nullable().optional(),
    gloss: z.string().nullable().optional(),
    senses: z.array(senseLinkSchema),
  })
  .strict();
export const derivationViewSchema = z
  .object({
    schema: z.literal(UI_SCHEMA),
    node: z.string(),
    family: z
      .object({
        root: z.string(),
        nodes: z.array(derivationNodeSchema),
        dangling_origins: z.array(z.string()),
      })
      .strict(),
  })
  .strict();
export type DerivationView = z.infer<typeof derivationViewSchema>;

export type LexiconQuery = {
  category?: string;
  gloss_contains?: string;
  sort?: "name" | "form" | "gloss";
};

export type ProposalQuery = {
  name: string;
  gloss?: string;
  categories: string[];
  template: string;
  count: number;
  seed: number;
  weights: SegmentWeight[];
};

export type GroupingQuery = { view: string; threshold: number };
