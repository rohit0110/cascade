import { DataSourcePlugin } from "@grafana/data";
import { ConfigEditor } from "./ConfigEditor";
import { QueryEditor } from "./QueryEditor";
import { CascadeDataSource } from "./datasource";
import type { CascadeOptions, CascadeQuery } from "./types";

export const plugin = new DataSourcePlugin<CascadeDataSource, CascadeQuery, CascadeOptions>(CascadeDataSource)
  .setConfigEditor(ConfigEditor)
  .setQueryEditor(QueryEditor);
