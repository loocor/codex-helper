#import <Foundation/Foundation.h>
#import <ServiceManagement/ServiceManagement.h>

#include <stdlib.h>
#include <string.h>

static int copy_error(char **error_message, NSString *message) {
  if (error_message == NULL) {
    return 1;
  }
  const char *utf8 = message.UTF8String;
  if (utf8 == NULL) {
    utf8 = "Failed to update login item";
  }
  char *copied = strdup(utf8);
  if (copied == NULL) {
    return 1;
  }
  *error_message = copied;
  return 1;
}

int codex_helper_smapp_set_enabled(int enabled, char **error_message) {
  if (@available(macOS 13.0, *)) {
    SMAppService *service = SMAppService.mainAppService;
    SMAppServiceStatus status = service.status;
    NSError *error = nil;

    if (enabled) {
      if (status == SMAppServiceStatusEnabled) {
        return 0;
      }
      if (![service registerAndReturnError:&error]) {
        NSString *message = error.localizedDescription ?: @"Failed to register login item";
        return copy_error(error_message, message);
      }
      status = service.status;
      if (status == SMAppServiceStatusEnabled) {
        return 0;
      }
      if (status == SMAppServiceStatusRequiresApproval) {
        return copy_error(
            error_message,
            @"Allow Codex Helper in System Settings > General > Login Items");
      }
      return copy_error(error_message, @"Login item registered but is not enabled");
    }

    if (status == SMAppServiceStatusNotRegistered) {
      return 0;
    }
    if (![service unregisterAndReturnError:&error]) {
      NSString *message = error.localizedDescription ?: @"Failed to unregister login item";
      return copy_error(error_message, message);
    }
    return 0;
  }
  return copy_error(error_message, @"Start at login requires macOS 13 or later");
}

void codex_helper_smapp_free(char *message) { free(message); }
