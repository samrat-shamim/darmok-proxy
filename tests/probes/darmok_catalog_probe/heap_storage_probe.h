/* Copyright 2026 Darmok contributors. SPDX-License-Identifier: Apache-2.0 */
#ifndef DARMOK_HEAP_STORAGE_PROBE_H
#define DARMOK_HEAP_STORAGE_PROBE_H
#include "tcop/utility.h"
extern void darmok_heap_storage_probe_command(const char *value);
extern bool darmok_heap_storage_probe_show(const char *name, DestReceiver *dest,
										   QueryCompletion *completion);
extern void darmok_heap_storage_probe_define_guc(void);
#endif
