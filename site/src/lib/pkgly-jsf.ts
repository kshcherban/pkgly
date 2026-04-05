export interface RootSchema {
  type?: string;
  title?: string;
  properties?: Record<string, FieldSchema>;
  required?: string[];
}

interface FieldSchema {
  type?: string;
  title?: string;
  default?: unknown;
  enum?: unknown[];
  oneOf?: Array<{ title?: string; const?: unknown }>;
}

type FieldType = "string" | "enum" | "boolean";

interface FormInputBase {
  key(): string;
  title(): string | undefined;
  default(): unknown;
  type(): FieldType;
}

export interface EnumValue {
  title?: string;
  value: unknown;
}

export class EnumInput implements FormInputBase {
  public readonly values: EnumValue[];

  constructor(
    private readonly fieldKey: string,
    private readonly fieldTitle: string | undefined,
    private readonly defaultValue: unknown,
    values: EnumValue[]
  ) {
    this.values = values;
  }

  key(): string {
    return this.fieldKey;
  }

  title(): string | undefined {
    return this.fieldTitle;
  }

  default(): unknown {
    return this.defaultValue;
  }

  type(): FieldType {
    return "enum";
  }
}

class FieldInput implements FormInputBase {
  constructor(
    private readonly fieldKey: string,
    private readonly fieldTitle: string | undefined,
    private readonly defaultValue: unknown,
    private readonly fieldType: FieldType
  ) {}

  key(): string {
    return this.fieldKey;
  }

  title(): string | undefined {
    return this.fieldTitle;
  }

  default(): unknown {
    return this.defaultValue;
  }

  type(): FieldType {
    return this.fieldType;
  }
}

export type FormInputType = FieldInput | EnumInput;

export class SchemaForm {
  constructor(private readonly fields: FormInputType[]) {}

  getProperties(_currentValue?: unknown): FormInputType[] {
    return this.fields;
  }
}

export function createForm(schema: RootSchema): SchemaForm {
  const requiredFields = new Set(schema.required ?? []);
  const fields = Object.entries(schema.properties ?? {}).map(([key, field]) => {
    return toFormInput(key, field, requiredFields.has(key));
  });
  return new SchemaForm(fields);
}

function toFormInput(key: string, field: FieldSchema, required: boolean): FormInputType {
  const enumValues = extractEnumValues(field);
  if (enumValues.length > 0) {
    const fallback = enumValues[0]?.value ?? "";
    const defaultValue = field.default ?? (required ? fallback : undefined);
    return new EnumInput(key, field.title, defaultValue, enumValues);
  }

  if (field.type === "boolean") {
    return new FieldInput(key, field.title, field.default ?? false, "boolean");
  }

  return new FieldInput(key, field.title, field.default ?? "", "string");
}

function extractEnumValues(field: FieldSchema): EnumValue[] {
  if (Array.isArray(field.oneOf) && field.oneOf.length > 0) {
    return field.oneOf
      .filter((value) => Object.prototype.hasOwnProperty.call(value, "const"))
      .map((value) => ({
        title: value.title,
        value: value.const,
      }));
  }

  if (Array.isArray(field.enum) && field.enum.length > 0) {
    return field.enum.map((value) => ({
      value,
      title: typeof value === "string" ? value : undefined,
    }));
  }

  return [];
}
