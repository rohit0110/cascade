import { InlineField, RadioButtonGroup } from "@grafana/ui";
import type { QueryEditorProps } from "@grafana/data";
import type { CascadeDataSource } from "./datasource";
import type { CascadeOptions, CascadeQuery, QueryKind } from "./types";

const kinds: { label: string; value: QueryKind }[] = [
  { label: "Impact graph", value: "nodegraph" },
  { label: "Feature table", value: "impact" },
];

export function QueryEditor({ query, onChange, onRunQuery }: QueryEditorProps<CascadeDataSource, CascadeQuery, CascadeOptions>) {
  return (
    <InlineField label="Query">
      <RadioButtonGroup
        options={kinds}
        value={query.kind ?? "nodegraph"}
        onChange={(kind) => {
          onChange({ ...query, kind });
          onRunQuery();
        }}
      />
    </InlineField>
  );
}
