{{/* The labels of the object `name`: the workload it belongs to, the release and the chart. */}}
{{- define "ultragraphdb.labels" -}}
app.kubernetes.io/name: {{ .name }}
app.kubernetes.io/instance: {{ .root.Release.Name }}
app.kubernetes.io/part-of: ultragraphdb
app.kubernetes.io/managed-by: {{ .root.Release.Service }}
helm.sh/chart: {{ printf "%s-%s" .root.Chart.Name .root.Chart.Version }}
{{- end }}

{{/* What selects the pods of workload `name`, which never changes, as a StatefulSet's selector cannot. */}}
{{- define "ultragraphdb.selector" -}}
app.kubernetes.io/name: {{ .name }}
{{- end }}

{{/* The pod security every role runs under, as user `user`. */}}
{{- define "ultragraphdb.podSecurity" -}}
securityContext:
  runAsNonRoot: true
  runAsUser: {{ .user }}
  runAsGroup: {{ .user }}
  fsGroup: {{ .user }}
  seccompProfile:
    {{- toYaml .root.Values.seccompProfile | nindent 4 }}
{{- end }}

{{/* A container's security: read-only, no capability, no privilege escalation. */}}
{{- define "ultragraphdb.containerSecurity" -}}
securityContext:
  allowPrivilegeEscalation: false
  readOnlyRootFilesystem: true
  capabilities:
    drop: ["ALL"]
{{- end }}

{{/* The five bootstrap variables of the role instance `name`. */}}
{{- define "ultragraphdb.bootstrap" -}}
- name: ULTRAGRAPHDB_CONFIG_URL
  value: {{ printf "http://config.%s.svc.%s:8090" .root.Release.Namespace .root.Values.clusterDomain | quote }}
- name: ULTRAGRAPHDB_CONFIG_TARGET
  value: kube
- name: ULTRAGRAPHDB_CONFIG_INSTANCE
  value: {{ .name }}
- name: ULTRAGRAPHDB_CONFIG_KEY_FILE
  value: /secrets/read.key
{{- end }}

{{/* The data directory's volume claim. */}}
{{- define "ultragraphdb.dataClaim" -}}
volumeClaimTemplates:
  - metadata:
      name: data
    spec:
      {{- with .Values.persistence.storageClassName }}
      storageClassName: {{ . }}
      {{- end }}
      accessModes: ["ReadWriteOnce"]
      resources:
        requests:
          storage: {{ .Values.persistence.size }}
{{- end }}

{{/* The probes on `port`: started and alive while it answers /health, ready while it answers /ready. */}}
{{- define "ultragraphdb.probes" -}}
startupProbe:
  httpGet: {path: /health, port: {{ .port }}}
  periodSeconds: 2
  failureThreshold: {{ .startup }}
livenessProbe:
  httpGet: {path: /health, port: {{ .port }}}
  periodSeconds: 10
readinessProbe:
  httpGet: {path: /ready, port: {{ .port }}}
  periodSeconds: 2
{{- end }}
