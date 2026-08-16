export interface GroupRels {
  group_name: string;
  col2: unknown;
}

export interface RelCatalog {
  relation_name: string;
  group_name: string;
  column_text: string;
  documentation_text: string;
}
