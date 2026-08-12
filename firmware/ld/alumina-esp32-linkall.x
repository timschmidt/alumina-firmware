/*
 * SPDX-License-Identifier: MIT OR Apache-2.0
 *
 * This is the esp-hal 1.0 classic-ESP32 aggregate linker composition with one
 * explicit text-order constraint. Its __pre_init shim emits a range-limited
 * direct call8 to the private esp32_init function. Large complete images can
 * otherwise place those input sections more than one call region apart.
 *
 * Keep the includes synchronized with esp-hal's esp32/linkall.x and esp32.x
 * while the 1.0 stack is selected. esp-hal 1.1 uses an indirect call instead;
 * dependency migration should remove this pinned-version composition.
 */

INCLUDE "memory.x"
INCLUDE "alias.x"
INCLUDE "exception.x"
INCLUDE "fixups/rtc_fast_rwdata_dummy.x"

SECTIONS {
  INCLUDE "rwtext.x"
  INCLUDE "rwdata.x"
}

INCLUDE "rodata.x"

SECTIONS {
  .text : ALIGN(4)
  {
    *(.literal .text)
    KEEP(*(.text.__post_init))
    KEEP(*(.text.__pre_init))
    KEEP(*(.literal._ZN7esp_hal3soc6xtensa10esp32_init*))
    KEEP(*(.text._ZN7esp_hal3soc6xtensa10esp32_init*))
    *(.literal.* .text.*)
  } > ROTEXT
}

ASSERT(
  SIZEOF(.text) > 0,
  "classic ESP32 text composition unexpectedly produced an empty image"
);

INCLUDE "rtc_fast.x"
INCLUDE "rtc_slow.x"
INCLUDE "stack.x"
INCLUDE "dram2.x"
INCLUDE "metadata.x"
INCLUDE "eh_frame.x"
INCLUDE "hal-defaults.x"
