/**
 * Custom hooks wrapping TanStack Query for admin API operations.
 */

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import * as api from '../api/client';
import type { ListParams, CreateObjectRequest, UpdateObjectRequest } from '../types/api';

// ── Query Keys ──────────────────────────────────────────────────────

export const queryKeys = {
  modelIndex: ['modelIndex'] as const,
  modelSchema: (app: string, model: string) =>
    ['modelSchema', app, model] as const,
  objectList: (app: string, model: string, params: ListParams) =>
    ['objectList', app, model, params] as const,
  objectDetail: (app: string, model: string, pk: string) =>
    ['objectDetail', app, model, pk] as const,
  recentActions: (limit: number) => ['recentActions', limit] as const,
  currentUser: ['currentUser'] as const,
};

// ── Model Index ─────────────────────────────────────────────────────

export function useModelIndex() {
  return useQuery({
    queryKey: queryKeys.modelIndex,
    queryFn: api.getModelIndex,
    staleTime: 5 * 60 * 1000,
  });
}

// ── Model Schema ────────────────────────────────────────────────────

export function useModelSchema(appLabel: string, modelName: string) {
  return useQuery({
    queryKey: queryKeys.modelSchema(appLabel, modelName),
    queryFn: () => api.getModelSchema(appLabel, modelName),
    staleTime: 10 * 60 * 1000,
    enabled: !!appLabel && !!modelName,
  });
}

// ── Object List ─────────────────────────────────────────────────────

export function useObjectList(
  appLabel: string,
  modelName: string,
  params: ListParams = {},
) {
  return useQuery({
    queryKey: queryKeys.objectList(appLabel, modelName, params),
    queryFn: () => api.listObjects(appLabel, modelName, params),
    enabled: !!appLabel && !!modelName,
  });
}

// ── Object Detail ───────────────────────────────────────────────────

export function useObjectDetail(
  appLabel: string,
  modelName: string,
  pk: string,
) {
  return useQuery({
    queryKey: queryKeys.objectDetail(appLabel, modelName, pk),
    queryFn: () => api.getObject(appLabel, modelName, pk),
    enabled: !!appLabel && !!modelName && !!pk,
  });
}

// ── Create Object ───────────────────────────────────────────────────

export function useCreateObject(appLabel: string, modelName: string) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (data: CreateObjectRequest) =>
      api.createObject(appLabel, modelName, data),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: ['objectList', appLabel, modelName],
      });
      queryClient.invalidateQueries({
        queryKey: queryKeys.recentActions(10),
      });
    },
  });
}

// ── Update Object ───────────────────────────────────────────────────

export function useUpdateObject(
  appLabel: string,
  modelName: string,
  pk: string,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (data: UpdateObjectRequest) =>
      api.updateObject(appLabel, modelName, pk, data),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: queryKeys.objectDetail(appLabel, modelName, pk),
      });
      queryClient.invalidateQueries({
        queryKey: ['objectList', appLabel, modelName],
      });
      queryClient.invalidateQueries({
        queryKey: queryKeys.recentActions(10),
      });
    },
  });
}

// ── Delete Object ───────────────────────────────────────────────────

export function useDeleteObject(appLabel: string, modelName: string) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (pk: string) => api.deleteObject(appLabel, modelName, pk),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: ['objectList', appLabel, modelName],
      });
      queryClient.invalidateQueries({
        queryKey: queryKeys.recentActions(10),
      });
    },
  });
}

// ── Recent Actions ──────────────────────────────────────────────────

export function useRecentActions(limit = 10) {
  return useQuery({
    queryKey: queryKeys.recentActions(limit),
    queryFn: () => api.getRecentActions(limit),
    staleTime: 30 * 1000,
  });
}
