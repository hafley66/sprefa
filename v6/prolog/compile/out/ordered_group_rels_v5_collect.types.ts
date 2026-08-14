export interface GroupRels {
  group_name: string;
  col2: unknown;
}

export interface RelCatalog {
  relation_name: string;
  group_name: string;
  _column_text: string;
  _documentation_text: string;
}
