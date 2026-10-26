import { DataSourceHttpSettings } from "@grafana/ui";
import type { DataSourcePluginOptionsEditorProps } from "@grafana/data";
import type { CascadeOptions } from "./types";

export function ConfigEditor({ options, onOptionsChange }: DataSourcePluginOptionsEditorProps<CascadeOptions>) {
  return (
    <DataSourceHttpSettings
      defaultUrl="http://cascade:7070"
      dataSourceConfig={options}
      onChange={onOptionsChange}
    />
  );
}
