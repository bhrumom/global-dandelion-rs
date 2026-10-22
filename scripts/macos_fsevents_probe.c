/* Test-only bounded historical observation. Not linked into the production CLI. */
#include <CoreServices/CoreServices.h>
#include <sys/stat.h>
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

struct observation {
    uint64_t count, maximum;
    uint32_t flags;
    int history_done, limited;
};

static void observe(ConstFSEventStreamRef stream, void *context, size_t count,
                    void *paths, const FSEventStreamEventFlags flags[],
                    const FSEventStreamEventId ids[]) {
    (void)stream;
    (void)paths;
    struct observation *out = context;
    for (size_t i = 0; i < count; ++i) {
        if (out->count++ >= 4096) { out->limited = 1; return; }
        out->flags |= flags[i];
        if (ids[i] > out->maximum) out->maximum = ids[i];
        if (flags[i] & kFSEventStreamEventFlagHistoryDone) out->history_done = 1;
    }
}

int main(int argc, char **argv) {
    if (argc != 3 && argc != 4) return 2;
    int host = argc == 4 && !strcmp(argv[3], "host");
    if (argc == 4 && !host) return 2;
    struct stat info;
    if (stat(argv[1], &info)) return 3;
    CFUUIDRef uuid = FSEventsCopyUUIDForDevice(info.st_dev);
    char text[64] = {0};
    if (uuid) {
        CFStringRef value = CFUUIDCreateString(NULL, uuid);
        if (!value) { CFRelease(uuid); return 4; }
        Boolean copied = CFStringGetCString(value, text, sizeof(text), kCFStringEncodingUTF8);
        CFRelease(value); CFRelease(uuid);
        if (!copied) return 4;
    }
    uint64_t cursor = FSEventsGetCurrentEventId();
    if (!strcmp(argv[2], "cursor")) {
        printf("{\"cursor\":%" PRIu64 ",\"uuid\":\"%s\"}\n", cursor, text);
        return 0;
    }
    char *end = NULL;
    uint64_t since = strtoull(argv[2], &end, 10);
    if (!end || *end) return 5;
    CFStringRef relative = host
        ? CFStringCreateWithFileSystemRepresentation(NULL, argv[1])
        : CFStringCreateCopy(NULL, CFSTR(""));
    if (!relative) return 6;
    CFArrayRef paths = CFArrayCreate(NULL, (const void **)&relative, 1, &kCFTypeArrayCallBacks);
    CFRelease(relative);
    if (!paths) return 6;
    struct observation out = {0};
    FSEventStreamContext context = {0, &out, NULL, NULL, NULL};
    FSEventStreamCreateFlags options = kFSEventStreamCreateFlagNoDefer |
        kFSEventStreamCreateFlagWatchRoot | kFSEventStreamCreateFlagFileEvents;
    FSEventStreamRef stream = host
        ? FSEventStreamCreate(NULL, observe, &context, paths, since, 0.1, options)
        : FSEventStreamCreateRelativeToDevice(NULL, observe, &context,
            info.st_dev, paths, since, 0.1, options);
    CFRelease(paths);
    if (!stream) return 7;
    CFRunLoopRef loop = CFRunLoopGetCurrent();
    FSEventStreamScheduleWithRunLoop(stream, loop, kCFRunLoopDefaultMode);
    Boolean started = FSEventStreamStart(stream);
    if (started) {
        CFAbsoluteTime deadline = CFAbsoluteTimeGetCurrent() + 8.0;
        while (!out.history_done && !out.limited && CFAbsoluteTimeGetCurrent() < deadline)
            CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.1, true);
        FSEventStreamStop(stream);
    }
    FSEventStreamUnscheduleFromRunLoop(stream, loop, kCFRunLoopDefaultMode);
    FSEventStreamInvalidate(stream);
    FSEventStreamRelease(stream);
    printf("{\"since\":%" PRIu64 ",\"current\":%" PRIu64 ",\"uuid\":\"%s\","
           "\"started\":%s,\"historyDone\":%s,\"limited\":%s,\"events\":%" PRIu64 ","
           "\"maximum\":%" PRIu64 ",\"flags\":%u,\"mountOrUnmount\":%s,\"historyUncertain\":%s}\n",
        since, cursor, text, started ? "true" : "false", out.history_done ? "true" : "false",
        out.limited ? "true" : "false", out.count, out.maximum, out.flags,
        out.flags & (kFSEventStreamEventFlagMount | kFSEventStreamEventFlagUnmount) ? "true" : "false",
        out.flags & (kFSEventStreamEventFlagMustScanSubDirs | kFSEventStreamEventFlagUserDropped |
                     kFSEventStreamEventFlagKernelDropped | kFSEventStreamEventFlagEventIdsWrapped |
                     kFSEventStreamEventFlagRootChanged) ? "true" : "false");
    return started ? 0 : 8;
}
