import { useState, useMemo, useCallback } from 'react';
import { useParams, useNavigate, Link } from 'react-router-dom';
import { useModelSchema, useObjectList } from '../hooks/useAdminApi';
import LoadingSpinner from '../components/LoadingSpinner';
import ErrorAlert from '../components/ErrorAlert';
import Breadcrumbs from '../components/Breadcrumbs';
import Pagination from '../components/Pagination';
import type { ListParams } from '../types/api';

export default function ModelListPage() {
  const { app, model } = useParams<{ app: string; model: string }>();
  const navigate = useNavigate();

  const [page, setPage] = useState(1);
  const [search, setSearch] = useState('');
  const [searchInput, setSearchInput] = useState('');
  const [ordering, setOrdering] = useState<string | undefined>(undefined);
  const [filters, setFilters] = useState<Record<string, string>>({});

  const appLabel = app ?? '';
  const modelName = model ?? '';

  const {
    data: schema,
    isLoading: schemaLoading,
    error: schemaError,
  } = useModelSchema(appLabel, modelName);

  const params: ListParams = useMemo(() => {
    const p: ListParams = {
      page,
      page_size: schema?.list_per_page ?? 25,
    };
    if (search) p.search = search;
    if (ordering) p.ordering = ordering;
    // Spread filter key-value pairs
    for (const [key, value] of Object.entries(filters)) {
      if (value) p[key] = value;
    }
    return p;
  }, [page, search, ordering, filters, schema?.list_per_page]);

  const {
    data: listData,
    isLoading: listLoading,
    error: listError,
    refetch,
  } = useObjectList(appLabel, modelName, params);

  const columns = useMemo(() => {
    if (!schema) return [];
    const display = schema.list_display.filter((c) => c !== '__str__');
    return display.length > 0 ? display : schema.fields.map((f) => f.name).slice(0, 5);
  }, [schema]);

  const handleSearch = useCallback(
    (e: React.FormEvent) => {
      e.preventDefault();
      setSearch(searchInput);
      setPage(1);
    },
    [searchInput],
  );

  const toggleOrdering = useCallback(
    (field: string) => {
      setOrdering((prev) => {
        if (prev === field) return `-${field}`;
        if (prev === `-${field}`) return undefined;
        return field;
      });
      setPage(1);
    },
    [],
  );

  const handleFilterChange = useCallback(
    (field: string, value: string) => {
      setFilters((prev) => {
        const next = { ...prev };
        if (value) {
          next[field] = value;
        } else {
          delete next[field];
        }
        return next;
      });
      setPage(1);
    },
    [],
  );

  // Find PK field
  const pkField = schema?.fields.find((f) => f.primary_key)?.name ?? 'id';

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

  function getSortIcon(field: string) {
    if (ordering === field) {
      return (
        <svg className="ml-1 inline h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" d="M4.5 15.75l7.5-7.5 7.5 7.5" />
        </svg>
      );
    }
    if (ordering === `-${field}`) {
      return (
        <svg className="ml-1 inline h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" d="M19.5 8.25l-7.5 7.5-7.5-7.5" />
        </svg>
      );
    }
    return (
      <svg className="ml-1 inline h-3.5 w-3.5 text-gray-300" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor">
        <path strokeLinecap="round" strokeLinejoin="round" d="M8.25 15L12 18.75 15.75 15m-7.5-6L12 5.25 15.75 9" />
      </svg>
    );
  }

  const hasActiveFilters = Object.values(filters).some(Boolean);

  return (
    <div className="space-y-4">
      {/* Breadcrumbs */}
      <Breadcrumbs
        items={[
          { label: appLabel, to: '/' },
          { label: schema.verbose_name_plural },
        ]}
      />

      {/* Header row */}
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <h1 className="text-2xl font-bold capitalize text-gray-900">
          {schema.verbose_name_plural}
        </h1>
        <Link
          to={`/${appLabel}/${modelName}/add`}
          className="inline-flex items-center gap-2 rounded-lg bg-indigo-600 px-4 py-2 text-sm font-semibold text-white shadow-sm transition-colors hover:bg-indigo-700"
        >
          <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" d="M12 4.5v15m7.5-7.5h-15" />
          </svg>
          Add {schema.verbose_name}
        </Link>
      </div>

      <div className="flex flex-col gap-4 lg:flex-row">
        {/* Main table area */}
        <div className="flex-1 space-y-4">
          {/* Search bar */}
          {schema.search_fields.length > 0 && (
            <form onSubmit={handleSearch} className="flex gap-2">
              <div className="relative flex-1">
                <svg
                  className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-gray-400"
                  fill="none"
                  viewBox="0 0 24 24"
                  strokeWidth={2}
                  stroke="currentColor"
                >
                  <path strokeLinecap="round" strokeLinejoin="round" d="M21 21l-5.197-5.197m0 0A7.5 7.5 0 105.196 5.196a7.5 7.5 0 0010.607 10.607z" />
                </svg>
                <input
                  type="text"
                  value={searchInput}
                  onChange={(e) => setSearchInput(e.target.value)}
                  placeholder={`Search ${schema.search_fields.join(', ')}...`}
                  className="block w-full rounded-lg border border-gray-300 bg-white py-2 pl-10 pr-3 text-sm shadow-sm placeholder:text-gray-400 focus:border-indigo-500 focus:outline-none focus:ring-1 focus:ring-indigo-500"
                />
              </div>
              <button
                type="submit"
                className="rounded-lg border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-700 shadow-sm hover:bg-gray-50"
              >
                Search
              </button>
              {search && (
                <button
                  type="button"
                  onClick={() => {
                    setSearch('');
                    setSearchInput('');
                    setPage(1);
                  }}
                  className="rounded-lg border border-gray-300 bg-white px-3 py-2 text-sm text-gray-500 hover:bg-gray-50"
                >
                  Clear
                </button>
              )}
            </form>
          )}

          {/* Active filter badges */}
          {(search || hasActiveFilters) && (
            <div className="flex flex-wrap gap-2">
              {search && (
                <span className="inline-flex items-center gap-1 rounded-full bg-indigo-100 px-3 py-1 text-xs font-medium text-indigo-800">
                  Search: "{search}"
                  <button
                    onClick={() => {
                      setSearch('');
                      setSearchInput('');
                    }}
                    className="ml-1 hover:text-indigo-600"
                  >
                    x
                  </button>
                </span>
              )}
              {Object.entries(filters).map(
                ([key, value]) =>
                  value && (
                    <span
                      key={key}
                      className="inline-flex items-center gap-1 rounded-full bg-purple-100 px-3 py-1 text-xs font-medium text-purple-800"
                    >
                      {key}: {value}
                      <button
                        onClick={() => handleFilterChange(key, '')}
                        className="ml-1 hover:text-purple-600"
                      >
                        x
                      </button>
                    </span>
                  ),
              )}
            </div>
          )}

          {/* Table */}
          <div className="overflow-hidden rounded-xl border border-gray-200 bg-white shadow-sm">
            {listLoading ? (
              <LoadingSpinner size="md" className="py-16" />
            ) : listError ? (
              <div className="p-4">
                <ErrorAlert
                  message="Failed to load objects"
                  onRetry={() => refetch()}
                />
              </div>
            ) : (
              <>
                <div className="overflow-x-auto">
                  <table className="w-full text-left text-sm">
                    <thead className="border-b border-gray-200 bg-gray-50">
                      <tr>
                        {columns.map((col) => (
                          <th
                            key={col}
                            className="whitespace-nowrap px-4 py-3 text-xs font-semibold uppercase tracking-wider text-gray-600"
                          >
                            <button
                              onClick={() => toggleOrdering(col)}
                              className="inline-flex items-center hover:text-gray-900"
                            >
                              {col.replace(/_/g, ' ')}
                              {getSortIcon(col)}
                            </button>
                          </th>
                        ))}
                        <th className="px-4 py-3" />
                      </tr>
                    </thead>
                    <tbody className="divide-y divide-gray-100">
                      {listData && listData.results.length > 0 ? (
                        listData.results.map((obj, idx) => {
                          const pk = String(obj[pkField] ?? idx);
                          return (
                            <tr
                              key={pk}
                              className="cursor-pointer transition-colors hover:bg-gray-50"
                              onClick={() =>
                                navigate(
                                  `/${appLabel}/${modelName}/${pk}/edit`,
                                )
                              }
                            >
                              {columns.map((col) => (
                                <td
                                  key={col}
                                  className="whitespace-nowrap px-4 py-3 text-gray-700"
                                >
                                  {renderCellValue(obj[col])}
                                </td>
                              ))}
                              <td className="px-4 py-3 text-right">
                                <svg
                                  className="h-4 w-4 text-gray-400"
                                  fill="none"
                                  viewBox="0 0 24 24"
                                  strokeWidth={1.5}
                                  stroke="currentColor"
                                >
                                  <path
                                    strokeLinecap="round"
                                    strokeLinejoin="round"
                                    d="M8.25 4.5l7.5 7.5-7.5 7.5"
                                  />
                                </svg>
                              </td>
                            </tr>
                          );
                        })
                      ) : (
                        <tr>
                          <td
                            colSpan={columns.length + 1}
                            className="px-4 py-12 text-center text-gray-500"
                          >
                            {search || hasActiveFilters
                              ? 'No results match your search or filters.'
                              : `No ${schema.verbose_name_plural} found.`}
                          </td>
                        </tr>
                      )}
                    </tbody>
                  </table>
                </div>

                {/* Pagination */}
                {listData && listData.total_pages > 1 && (
                  <div className="border-t border-gray-200 px-4 py-3">
                    <Pagination
                      page={listData.page}
                      totalPages={listData.total_pages}
                      count={listData.count}
                      pageSize={listData.page_size}
                      onPageChange={setPage}
                    />
                  </div>
                )}
              </>
            )}
          </div>
        </div>

        {/* Filter sidebar */}
        {schema.fields.length > 0 && (
          <FilterSidebar
            schema={schema}
            filters={filters}
            onFilterChange={handleFilterChange}
          />
        )}
      </div>
    </div>
  );
}

// ── Helpers ──────────────────────────────────────────────────────────

function renderCellValue(value: unknown): string {
  if (value === null || value === undefined) return '-';
  if (typeof value === 'boolean') return value ? 'Yes' : 'No';
  if (typeof value === 'object') return JSON.stringify(value);
  return String(value);
}

interface FilterSidebarProps {
  schema: {
    fields: { name: string; field_type: string; choices: [string, string][] | null }[];
  };
  filters: Record<string, string>;
  onFilterChange: (field: string, value: string) => void;
}

function FilterSidebar({ schema, filters, onFilterChange }: FilterSidebarProps) {
  // Only show fields that have choices or are boolean
  const filterableFields = schema.fields.filter(
    (f) =>
      (f.choices && f.choices.length > 0) || f.field_type === 'BooleanField',
  );

  if (filterableFields.length === 0) return null;

  return (
    <aside className="w-full shrink-0 lg:w-56">
      <div className="rounded-xl border border-gray-200 bg-white p-4 shadow-sm">
        <h3 className="mb-3 text-sm font-semibold text-gray-900">Filters</h3>
        <div className="space-y-4">
          {filterableFields.map((field) => (
            <div key={field.name}>
              <label className="mb-1 block text-xs font-medium capitalize text-gray-600">
                {field.name.replace(/_/g, ' ')}
              </label>
              {field.field_type === 'BooleanField' ? (
                <select
                  value={filters[field.name] ?? ''}
                  onChange={(e) =>
                    onFilterChange(field.name, e.target.value)
                  }
                  className="block w-full rounded-lg border border-gray-300 bg-white px-2 py-1.5 text-sm focus:border-indigo-500 focus:outline-none"
                >
                  <option value="">All</option>
                  <option value="true">Yes</option>
                  <option value="false">No</option>
                </select>
              ) : (
                <select
                  value={filters[field.name] ?? ''}
                  onChange={(e) =>
                    onFilterChange(field.name, e.target.value)
                  }
                  className="block w-full rounded-lg border border-gray-300 bg-white px-2 py-1.5 text-sm focus:border-indigo-500 focus:outline-none"
                >
                  <option value="">All</option>
                  {field.choices?.map(([val, label]) => (
                    <option key={val} value={val}>
                      {label}
                    </option>
                  ))}
                </select>
              )}
            </div>
          ))}
        </div>
      </div>
    </aside>
  );
}
