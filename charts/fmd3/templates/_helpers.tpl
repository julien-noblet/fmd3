{{/*
Expand the name of the chart.
*/}}
{{- define "fmd3.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create a default fully qualified app name.
*/}}
{{- define "fmd3.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- if contains $name .Release.Name }}
{{- .Release.Name | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
{{- end }}
{{- end }}
{{- end }}

{{/*
Chart label
*/}}
{{- define "fmd3.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Common labels
*/}}
{{- define "fmd3.labels" -}}
helm.sh/chart: {{ include "fmd3.chart" . }}
{{ include "fmd3.selectorLabels" . }}
{{- if .Chart.AppVersion }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
{{- end }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end }}

{{/*
Selector labels
*/}}
{{- define "fmd3.selectorLabels" -}}
app.kubernetes.io/name: {{ include "fmd3.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{/*
NATS URL helper
*/}}
{{- define "fmd3.natsUrl" -}}
{{- if .Values.nats.customUrl }}
{{- .Values.nats.customUrl }}
{{- else }}
{{- printf "nats://%s-nats:%d" (include "fmd3.fullname" .) (int .Values.nats.port) }}
{{- end }}
{{- end }}
