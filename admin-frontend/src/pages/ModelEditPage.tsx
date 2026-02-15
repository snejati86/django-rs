import { useState, useCallback, useMemo } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import {
  useModelSchema,
  useObjectDetail,
  useUpdateObject,
  useDeleteObject,
} from '../hooks/useAdminApi';
import { useToast } from '../contexts/ToastContext';
import LoadingSpinner from '../components/LoadingSpinner';
import ErrorAlert from '../components/ErrorAlert';
import Breadcrumbs from '../components/Breadcrumbs';
import FormField from '../components/FormField';
import ConfirmDialog from '../components/ConfirmDialog';

export default function ModelEditPage() {
  const { app, model, pk } = useParams<{
    app: string;
    model: string;
    pk: string;
  }>();
  const navigate = useNavigate();
  const { addToast } = useToast();

  const appLabel = app ?? '';
  const modelName = model ?? '';
  const objectPk = pk ?? '';

  const {
    data: schema,
    isLoading: schemaLoading,
  } = useModelSchema(appLabel, modelName);

  const {
    data: objectData,
    isLoading: objectLoading,
    error: objectError,
    dataUpdatedAt,
  } = useObjectDetail(appLabel, modelName, objectPk);

  const updateMutation = useUpdateObject(appLabel, modelName, objectPk);
  const deleteMutation = useDeleteObject(appLabel, modelName);

  // Track local field overrides separately from server data
  const [localOverrides, setLocalOverrides] = useState<Record<string, unknown>>({});
  const [overridesBase, setOverridesBase] = useState(0);
  const [showDelete, setShowDelete] = useState(false);

  // Reset overrides when server data changes (e.g. after refetch)
  if (dataUpdatedAt !== 0 && overridesBase !== dataUpdatedAt) {
    setOverridesBase(dataUpdatedAt);
    setLocalOverrides({});
  }

  // Merge server data with local overrides
  const formData = useMemo(() => {
    if (!objectData) return {};
    return { ...objectData, ...localOverrides };
  }, [objectData, localOverrides]);

  const handleFieldChange = useCallback(
    (name: string, value: unknown) => {
      setLocalOverrides((prev) => ({ ...prev, [name]: value }));
    },
    [],
  );

  const handleSave = useCallback(
    async (e: React.FormEvent) => {
      e.preventDefault();
      try {
        await updateMutation.mutateAsync(formData);
        addToast(
          `${schema?.verbose_name ?? 'Object'} saved successfully`,
          'success',
        );
      } catch (err) {
        addToast(
          `Failed to save: ${err instanceof Error ? err.message : 'Unknown error'}`,
          'error',
        );
      }
    },
    [formData, updateMutation, addToast, schema],
  );

  const handleDelete = useCallback(async () => {
    setShowDelete(false);
    try {
      await deleteMutation.mutateAsync(objectPk);
      addToast(
        `${schema?.verbose_name ?? 'Object'} deleted successfully`,
        'success',
      );
      navigate(`/${appLabel}/${modelName}`);
    } catch (err) {
      addToast(
        `Failed to delete: ${err instanceof Error ? err.message : 'Unknown error'}`,
        'error',
      );
    }
  }, [deleteMutation, objectPk, addToast, schema, navigate, appLabel, modelName]);

  if (schemaLoading || objectLoading) {
    return <LoadingSpinner size="lg" className="mt-20" />;
  }

  if (objectError || !schema) {
    return (
      <div className="mx-auto max-w-xl mt-10">
        <ErrorAlert
          message={`Failed to load ${appLabel}.${modelName} #${objectPk}`}
          onRetry={() => window.location.reload()}
        />
      </div>
    );
  }

  // Editable fields (exclude primary key and read-only)
  const editableFields = schema.fields.filter(
    (f) => !f.primary_key && !f.read_only,
  );
  const readOnlyFields = schema.fields.filter(
    (f) => f.primary_key || f.read_only,
  );

  // Get the display name of the object
  const objectRepr =
    (formData['__str__'] as string) ??
    (formData['name'] as string) ??
    (formData['title'] as string) ??
    `#${objectPk}`;

  return (
    <div className="space-y-4">
      <Breadcrumbs
        items={[
          { label: appLabel, to: '/' },
          {
            label: schema.verbose_name_plural,
            to: `/${appLabel}/${modelName}`,
          },
          { label: objectRepr },
        ]}
      />

      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <h1 className="text-2xl font-bold text-gray-900">
          Edit {schema.verbose_name}
        </h1>
      </div>

      <form onSubmit={handleSave} className="space-y-6">
        {/* Read-only fields */}
        {readOnlyFields.length > 0 && (
          <div className="rounded-xl border border-gray-200 bg-white p-6 shadow-sm">
            <h2 className="mb-4 text-sm font-semibold uppercase tracking-wider text-gray-500">
              Read-only fields
            </h2>
            <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
              {readOnlyFields.map((field) => (
                <FormField
                  key={field.name}
                  field={field}
                  value={formData[field.name]}
                  onChange={handleFieldChange}
                  disabled
                />
              ))}
            </div>
          </div>
        )}

        {/* Editable fields */}
        <div className="rounded-xl border border-gray-200 bg-white p-6 shadow-sm">
          <div className="grid gap-5 sm:grid-cols-2">
            {editableFields.map((field) => (
              <div
                key={field.name}
                className={
                  field.field_type === 'TextField' ? 'sm:col-span-2' : ''
                }
              >
                <FormField
                  field={field}
                  value={formData[field.name]}
                  onChange={handleFieldChange}
                />
              </div>
            ))}
          </div>
        </div>

        {/* Action buttons */}
        <div className="flex items-center justify-between rounded-xl border border-gray-200 bg-white px-6 py-4 shadow-sm">
          <button
            type="button"
            onClick={() => setShowDelete(true)}
            className="rounded-lg border border-red-300 bg-white px-4 py-2 text-sm font-medium text-red-600 transition-colors hover:bg-red-50"
          >
            Delete
          </button>
          <div className="flex gap-3">
            <button
              type="button"
              onClick={() => navigate(`/${appLabel}/${modelName}`)}
              className="rounded-lg border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-700 transition-colors hover:bg-gray-50"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={updateMutation.isPending}
              className="inline-flex items-center gap-2 rounded-lg bg-indigo-600 px-5 py-2 text-sm font-semibold text-white shadow-sm transition-colors hover:bg-indigo-700 disabled:opacity-60"
            >
              {updateMutation.isPending && (
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
        </div>
      </form>

      {/* Delete confirmation dialog */}
      <ConfirmDialog
        isOpen={showDelete}
        title={`Delete ${schema.verbose_name}`}
        message={`Are you sure you want to delete "${objectRepr}"? This action cannot be undone.`}
        confirmLabel="Delete"
        variant="danger"
        onConfirm={handleDelete}
        onCancel={() => setShowDelete(false)}
      />
    </div>
  );
}
