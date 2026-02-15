import type { FieldSchema } from '../types/api';

interface FormFieldProps {
  field: FieldSchema;
  value: unknown;
  onChange: (name: string, value: unknown) => void;
  disabled?: boolean;
}

/**
 * Renders a form input based on the field schema from the backend.
 * Maps Django field types to appropriate HTML input elements.
 */
export default function FormField({
  field,
  value,
  onChange,
  disabled = false,
}: FormFieldProps) {
  const isDisabled = disabled || field.read_only;
  const stringValue = value === null || value === undefined ? '' : String(value);

  const baseInputClass =
    'block w-full rounded-lg border border-gray-300 bg-white px-3 py-2 text-sm text-gray-900 shadow-sm transition-colors placeholder:text-gray-400 focus:border-indigo-500 focus:outline-none focus:ring-1 focus:ring-indigo-500 disabled:cursor-not-allowed disabled:bg-gray-100 disabled:text-gray-500';

  // Map field_type to input type/element
  function renderInput() {
    // Choice field
    if (field.choices && field.choices.length > 0) {
      return (
        <select
          id={field.name}
          name={field.name}
          value={stringValue}
          onChange={(e) => onChange(field.name, e.target.value)}
          disabled={isDisabled}
          className={baseInputClass}
        >
          {!field.required && <option value="">---------</option>}
          {field.choices.map(([val, label]) => (
            <option key={val} value={val}>
              {label}
            </option>
          ))}
        </select>
      );
    }

    switch (field.field_type) {
      case 'TextField':
        return (
          <textarea
            id={field.name}
            name={field.name}
            value={stringValue}
            onChange={(e) => onChange(field.name, e.target.value)}
            disabled={isDisabled}
            rows={4}
            className={baseInputClass}
            placeholder={field.help_text || undefined}
          />
        );

      case 'BooleanField':
        return (
          <label className="relative inline-flex cursor-pointer items-center gap-3">
            <input
              type="checkbox"
              id={field.name}
              name={field.name}
              checked={Boolean(value)}
              onChange={(e) => onChange(field.name, e.target.checked)}
              disabled={isDisabled}
              className="h-4 w-4 rounded border-gray-300 text-indigo-600 focus:ring-indigo-500"
            />
            <span className="text-sm text-gray-700">
              {value ? 'Yes' : 'No'}
            </span>
          </label>
        );

      case 'IntegerField':
      case 'BigIntegerField':
      case 'SmallIntegerField':
      case 'PositiveIntegerField':
      case 'BigAutoField':
      case 'AutoField':
        return (
          <input
            type="number"
            id={field.name}
            name={field.name}
            value={stringValue}
            onChange={(e) => onChange(field.name, e.target.value === '' ? null : Number(e.target.value))}
            disabled={isDisabled}
            className={baseInputClass}
            step="1"
          />
        );

      case 'FloatField':
      case 'DecimalField':
        return (
          <input
            type="number"
            id={field.name}
            name={field.name}
            value={stringValue}
            onChange={(e) => onChange(field.name, e.target.value === '' ? null : Number(e.target.value))}
            disabled={isDisabled}
            className={baseInputClass}
            step="any"
          />
        );

      case 'DateField':
        return (
          <input
            type="date"
            id={field.name}
            name={field.name}
            value={stringValue}
            onChange={(e) => onChange(field.name, e.target.value)}
            disabled={isDisabled}
            className={baseInputClass}
          />
        );

      case 'DateTimeField':
        return (
          <input
            type="datetime-local"
            id={field.name}
            name={field.name}
            value={stringValue}
            onChange={(e) => onChange(field.name, e.target.value)}
            disabled={isDisabled}
            className={baseInputClass}
          />
        );

      case 'TimeField':
        return (
          <input
            type="time"
            id={field.name}
            name={field.name}
            value={stringValue}
            onChange={(e) => onChange(field.name, e.target.value)}
            disabled={isDisabled}
            className={baseInputClass}
          />
        );

      case 'EmailField':
        return (
          <input
            type="email"
            id={field.name}
            name={field.name}
            value={stringValue}
            onChange={(e) => onChange(field.name, e.target.value)}
            disabled={isDisabled}
            className={baseInputClass}
            maxLength={field.max_length ?? undefined}
            placeholder={field.help_text || undefined}
          />
        );

      case 'URLField':
        return (
          <input
            type="url"
            id={field.name}
            name={field.name}
            value={stringValue}
            onChange={(e) => onChange(field.name, e.target.value)}
            disabled={isDisabled}
            className={baseInputClass}
            maxLength={field.max_length ?? undefined}
            placeholder={field.help_text || 'https://'}
          />
        );

      case 'SlugField':
      case 'CharField':
      default:
        return (
          <input
            type="text"
            id={field.name}
            name={field.name}
            value={stringValue}
            onChange={(e) => onChange(field.name, e.target.value)}
            disabled={isDisabled}
            className={baseInputClass}
            maxLength={field.max_length ?? undefined}
            placeholder={field.help_text || undefined}
          />
        );
    }
  }

  // BooleanField renders its own label inline
  if (field.field_type === 'BooleanField') {
    return (
      <div className="space-y-1">
        <label
          htmlFor={field.name}
          className="block text-sm font-medium text-gray-700"
        >
          <span className="capitalize">{field.label}</span>
          {field.required && <span className="ml-1 text-red-500">*</span>}
        </label>
        {renderInput()}
        {field.help_text && field.field_type === 'BooleanField' && (
          <p className="text-xs text-gray-500">{field.help_text}</p>
        )}
      </div>
    );
  }

  return (
    <div className="space-y-1">
      <label
        htmlFor={field.name}
        className="block text-sm font-medium text-gray-700"
      >
        <span className="capitalize">{field.label}</span>
        {field.required && !field.primary_key && (
          <span className="ml-1 text-red-500">*</span>
        )}
      </label>
      {renderInput()}
      {field.help_text && field.field_type !== 'BooleanField' && (
        <p className="text-xs text-gray-500">{field.help_text}</p>
      )}
      {field.max_length && (
        <p className="text-xs text-gray-400">
          Max {field.max_length} characters
        </p>
      )}
    </div>
  );
}
