import { Link } from 'react-router-dom';
import { useModelIndex, useRecentActions } from '../hooks/useAdminApi';
import LoadingSpinner from '../components/LoadingSpinner';
import ErrorAlert from '../components/ErrorAlert';
import type { LogEntry } from '../types/api';

const actionFlagColors: Record<string, string> = {
  Addition: 'bg-green-100 text-green-800',
  Change: 'bg-blue-100 text-blue-800',
  Deletion: 'bg-red-100 text-red-800',
};

function formatTimestamp(ts: string): string {
  try {
    const date = new Date(ts);
    return date.toLocaleString(undefined, {
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  } catch {
    return ts;
  }
}

function RecentActionRow({ entry }: { entry: LogEntry }) {
  const [app, model] = entry.content_type.split('.');
  const isDeleted = entry.action_flag === 'Deletion';

  return (
    <div className="flex items-start gap-3 py-3">
      <span
        className={`mt-0.5 inline-flex shrink-0 rounded-full px-2 py-0.5 text-xs font-medium ${
          actionFlagColors[entry.action_flag] ?? 'bg-gray-100 text-gray-700'
        }`}
      >
        {entry.action_flag}
      </span>
      <div className="min-w-0 flex-1">
        {isDeleted ? (
          <p className="text-sm text-gray-500 line-through">
            {entry.object_repr}
          </p>
        ) : (
          <Link
            to={`/${app}/${model}/${entry.object_id}/edit`}
            className="text-sm font-medium text-indigo-600 hover:text-indigo-800 hover:underline"
          >
            {entry.object_repr}
          </Link>
        )}
        <p className="mt-0.5 text-xs text-gray-500">
          {entry.content_type} &middot; {formatTimestamp(entry.action_time)}
          {entry.change_message && ` - ${entry.change_message}`}
        </p>
      </div>
    </div>
  );
}

export default function DashboardPage() {
  const {
    data: indexData,
    isLoading: indexLoading,
    error: indexError,
    refetch: refetchIndex,
  } = useModelIndex();

  const {
    data: recentActions,
    isLoading: actionsLoading,
  } = useRecentActions(10);

  if (indexLoading) {
    return <LoadingSpinner size="lg" className="mt-20" />;
  }

  if (indexError) {
    return (
      <div className="mx-auto max-w-xl mt-20">
        <ErrorAlert
          message="Failed to load admin dashboard. Is the backend running?"
          onRetry={() => refetchIndex()}
        />
      </div>
    );
  }

  const totalModels =
    indexData?.apps?.reduce((sum, app) => sum + app.models.length, 0) ?? 0;

  return (
    <div className="space-y-8">
      {/* Page Header */}
      <div>
        <h1 className="text-2xl font-bold text-gray-900">Dashboard</h1>
        <p className="mt-1 text-sm text-gray-500">
          Welcome to the django-rs administration panel.
        </p>
      </div>

      {/* Stats row */}
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <div className="rounded-xl border border-gray-200 bg-white p-5 shadow-sm">
          <div className="flex items-center gap-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-indigo-100">
              <svg className="h-5 w-5 text-indigo-600" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" d="M20.25 6.375c0 2.278-3.694 4.125-8.25 4.125S3.75 8.653 3.75 6.375m16.5 0c0-2.278-3.694-4.125-8.25-4.125S3.75 4.097 3.75 6.375m16.5 0v11.25c0 2.278-3.694 4.125-8.25 4.125s-8.25-1.847-8.25-4.125V6.375m16.5 0v3.75m-16.5-3.75v3.75m16.5 0v3.75C20.25 16.153 16.556 18 12 18s-8.25-1.847-8.25-4.125v-3.75m16.5 0c0 2.278-3.694 4.125-8.25 4.125s-8.25-1.847-8.25-4.125" />
              </svg>
            </div>
            <div>
              <p className="text-sm text-gray-500">Total models</p>
              <p className="text-2xl font-bold text-gray-900">{totalModels}</p>
            </div>
          </div>
        </div>
        <div className="rounded-xl border border-gray-200 bg-white p-5 shadow-sm">
          <div className="flex items-center gap-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-purple-100">
              <svg className="h-5 w-5 text-purple-600" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" d="M2.25 7.125C2.25 6.504 2.754 6 3.375 6h6c.621 0 1.125.504 1.125 1.125v3.75c0 .621-.504 1.125-1.125 1.125h-6a1.125 1.125 0 01-1.125-1.125v-3.75zM14.25 8.625c0-.621.504-1.125 1.125-1.125h5.25c.621 0 1.125.504 1.125 1.125v8.25c0 .621-.504 1.125-1.125 1.125h-5.25a1.125 1.125 0 01-1.125-1.125v-8.25zM2.25 16.5c0-.621.504-1.125 1.125-1.125h6c.621 0 1.125.504 1.125 1.125v2.25c0 .621-.504 1.125-1.125 1.125h-6a1.125 1.125 0 01-1.125-1.125v-2.25z" />
              </svg>
            </div>
            <div>
              <p className="text-sm text-gray-500">Applications</p>
              <p className="text-2xl font-bold text-gray-900">
                {indexData?.apps?.length ?? 0}
              </p>
            </div>
          </div>
        </div>
      </div>

      <div className="grid gap-6 lg:grid-cols-3">
        {/* Model cards */}
        <div className="lg:col-span-2">
          <h2 className="mb-4 text-lg font-semibold text-gray-900">
            Registered models
          </h2>
          <div className="grid gap-3 sm:grid-cols-2">
            {indexData?.apps?.map((app) =>
              app.models.map((model) => (
                <Link
                  key={`${app.app_label}.${model.name}`}
                  to={`/${app.app_label}/${model.name}`}
                  className="group flex items-center gap-4 rounded-xl border border-gray-200 bg-white p-4 shadow-sm transition-all hover:border-indigo-300 hover:shadow-md"
                >
                  <div className="flex h-12 w-12 shrink-0 items-center justify-center rounded-lg bg-gradient-to-br from-indigo-500 to-purple-600 text-lg font-bold text-white shadow">
                    {model.verbose_name.charAt(0).toUpperCase()}
                  </div>
                  <div className="min-w-0">
                    <p className="text-sm font-semibold text-gray-900 capitalize group-hover:text-indigo-700">
                      {model.verbose_name_plural}
                    </p>
                    <p className="text-xs text-gray-500">
                      {app.app_label}.{model.name}
                    </p>
                  </div>
                  <svg
                    className="ml-auto h-5 w-5 shrink-0 text-gray-400 transition-transform group-hover:translate-x-1 group-hover:text-indigo-500"
                    fill="none"
                    viewBox="0 0 24 24"
                    strokeWidth={1.5}
                    stroke="currentColor"
                  >
                    <path strokeLinecap="round" strokeLinejoin="round" d="M8.25 4.5l7.5 7.5-7.5 7.5" />
                  </svg>
                </Link>
              )),
            )}
          </div>
          {totalModels === 0 && (
            <div className="rounded-xl border border-dashed border-gray-300 bg-white p-8 text-center">
              <p className="text-sm text-gray-500">
                No models registered yet. Register models using{' '}
                <code className="rounded bg-gray-100 px-1.5 py-0.5 font-mono text-xs">
                  AdminSite::register()
                </code>
              </p>
            </div>
          )}
        </div>

        {/* Recent Actions sidebar */}
        <div>
          <h2 className="mb-4 text-lg font-semibold text-gray-900">
            Recent actions
          </h2>
          <div className="rounded-xl border border-gray-200 bg-white shadow-sm">
            {actionsLoading ? (
              <div className="p-6">
                <LoadingSpinner size="sm" />
              </div>
            ) : recentActions && recentActions.length > 0 ? (
              <div className="divide-y divide-gray-100 px-4">
                {recentActions.map((entry) => (
                  <RecentActionRow key={entry.id} entry={entry} />
                ))}
              </div>
            ) : (
              <div className="p-6 text-center">
                <svg className="mx-auto h-8 w-8 text-gray-300" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" d="M12 6v6h4.5m4.5 0a9 9 0 11-18 0 9 9 0 0118 0z" />
                </svg>
                <p className="mt-2 text-sm text-gray-500">
                  No recent actions
                </p>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
