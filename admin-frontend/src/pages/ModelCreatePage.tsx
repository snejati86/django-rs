import { useState, useCallback } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import { useModelSchema, useCreateObject } from '../hooks/useAdminApi';
import { useToast } from '../contexts/ToastContext';
import LoadingSpinner from '../components/LoadingSpinner';
import ErrorAlert from '../components/ErrorAlert';
import Breadcrumbs from '../components/Breadcrumbs';
import FormField from '../components/FormField';

export default function ModelCreatePage() {
  const { app, model } = useParams<{ app: string; model: string }>();
  const navigate = useNavigate();
  const { addToast } = useToast();

  const appLabel = app ?? '';
  const modelName = model ?? '';

  const {
    data: schema,
    isLoading: schemaLoading,
    error: schemaError,
  } = useModelSchema(appLabel, modelName);

  const createMutation = useCreateObject(appLabel, modelName);

  const [formData, setFormData] = useState<Record<string, unknown>>({});

  const handleFieldChange = useCallback(
    (name: string, value: unknown) => {
      setFormData((prev) => ({ ...prev, [name]: value }));
    },
    [],
  );

  const handleSubmit = useCallback(
    async (e: React.FormEvent) => {
      e.preventDefault();
      try {
        const created = await createMutation.mutateAsync(formData);
        addToast(
          `${schema?.verbose_name ?? 'Object'} created successfully`,
          'success',
        );
        // Navigate to the edit page of the newly created object
        const pkField =
          schema?.fields.find((f) => f.primary_key)?.name ?? 'id';
        const newPk = created[pkField];
        if (newPk !== undefined) {
          navigate(`/${appLabel}/${modelName}/${newPk}/edit`);
        } else {
          navigate(`/${appLabel}/${modelName}`);
        }
      } catch (err) {
        addToast(
          `Failed to create: ${err instanceof Error ? err.message : 'Unknown error'}`,
          'error',
        );
      }
    },
    [formData, createMutation, addToast, schema, navigate, appLabel, modelName],
  );

  if (schemaLoading) {
    return <LoadingSpinner size="lg" className="mt-20" />;
  }

  if (schemaError || !schema) {
    return (
      <div className="mx-auto max-w-xl mt-10">
        <ErrorAlert
          message={`Failed to load schema for ${appLabel}.${modelName}`}
          onRetry={() => window.location.reload()}
        />
      </div>
    );
  }

  // Only show non-primary-key, non-read-only fields for creation
  const creatableFields = schema.fields.filter(
    (f) => !f.primary_key && !f.read_only,
  );

  return (
    <div className="space-y-4">
      <Breadcrumbs
        items={[
          { label: appLabel, to: '/' },
          {
            label: schema.verbose_name_plural,
            to: `/${appLabel}/${modelName}`,
          },
          { label: `Add ${schema.verbose_name}` },
        ]}
      />

      <h1 className="text-2xl font-bold text-gray-900">
        Add {schema.verbose_name}
      </h1>

      <form onSubmit={handleSubmit} className="space-y-6">
        <div className="rounded-xl border border-gray-200 bg-white p-6 shadow-sm">
          {creatableFields.length === 0 ? (
            <p className="text-sm text-gray-500">
              No editable fields defined for this model.
            </p>
          ) : (
            <div className="grid gap-5 sm:grid-cols-2">
              {creatableFields.map((field) => (
                <div
                  key={field.name}
                  className={
                    field.field_type === 'TextField' ? 'sm:col-span-2' : ''
                  }
                >
                  <FormField
                    field={field}
                    value={formData[field.name] ?? (field.field_type === 'BooleanField' ? false : '')}
                    onChange={handleFieldChange}
                  />
                </div>
              ))}
            </div>
          )}
        </div>

        {/* Action buttons */}
        <div className="flex items-center justify-end gap-3 rounded-xl border border-gray-200 bg-white px-6 py-4 shadow-sm">
          <button
            type="button"
            onClick={() => navigate(`/${appLabel}/${modelName}`)}
            className="rounded-lg border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-700 transition-colors hover:bg-gray-50"
          >
            Cancel
          </button>
          <button
            type="submit"
            disabled={createMutation.isPending}
            className="inline-flex items-center gap-2 rounded-lg bg-indigo-600 px-5 py-2 text-sm font-semibold text-white shadow-sm transition-colors hover:bg-indigo-700 disabled:opacity-60"
          >
            {createMutation.isPending && (
              <svg
                className="h-4 w-4 animate-spin"
                fill="none"
                viewBox="0 0 24 24"
              >
                <circle
                  className="opacity-25"
                  cx="12"
                  cy="12"
                  r="10"
                  stroke="currentColor"
                  strokeWidth="4"
                />
                <path
                  className="opacity-75"
                  fill="currentColor"
                  d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"
                />
              </svg>
            )}
            Save
          </button>
        </div>
      </form>
    </div>
  );
}
