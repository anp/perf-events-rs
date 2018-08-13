use bytes::BytesMut;
use num::FromPrimitive;
use tokio_codec::Decoder;

use error::Error;
use raw::*;

pub struct RecordDecoder;

impl Decoder for RecordDecoder {
    type Item = Record;
    type Error = Error;

    fn decode(&mut self, _src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        unimplemented!();
    }
}

pub struct Record {
    _metadata: Metadata,
    _contents: RecordContents,
}

pub enum RecordContents {}

/// The mmap values start with a header.
struct EventHeader {
    event_type: SampledEventType,
    misc: Metadata,
    size: u16,
}

impl<'a> From<&'a perf_event_header> for EventHeader {
    fn from(raw: &perf_event_header) -> Self {
        Self {
            size: raw.size,
            event_type: SampledEventType::from_u32(raw.type_).unwrap(),
            misc: Metadata::from(raw.misc),
        }
    }
}

/// The misc field contains additional information about the sample.
///
/// Note: we do not support accessing the PERF_RECORD_MISC_PROC_MAP_PARSE_TIMEOUT
/// value. From the syscall's doc page:
///     This bit is not set by the kernel.  It is reserved for the user-space perf
///     utility to indicate that /proc/i[pid]/maps parsing was taking too long and was
///     stopped, and thus the mmap records may be truncated.
struct Metadata {
    _cpu_mode: CpuMode,
    /// Since the following three statuses are generated by different
    /// record types, they alias to the same bit, which is represented here as
    /// a bool:
    ///
    /// `PERF_RECORD_MISC_MMAP_DATA` (since Linux 3.10)
    ///        This is set when the mapping is not executable; other‐
    ///        wise the mapping is executable.
    ///
    /// `PERF_RECORD_MISC_COMM_EXEC` (since Linux 3.16)
    ///        This is set for a PERF_RECORD_COMM record on kernels
    ///        more recent than Linux 3.16 if a process name change
    ///        was caused by an exec(2) system call.
    ///
    /// `PERF_RECORD_MISC_SWITCH_OUT` (since Linux 4.3)
    ///        When a PERF_RECORD_SWITCH or
    ///        PERF_RECORD_SWITCH_CPU_WIDE record is generated, this
    ///        bit indicates that the context switch is away from the
    ///        current process (instead of into the current process).
    _multipurpose_lol: bool,
    /// This indicates that the content of PERF_SAMPLE_IP points to the actual instruction that
    /// triggered the event.  See also perf_event_attr.precise_ip. (PERF_RECORD_MISC_EXACT_IP)
    _exact_ip: bool,
    /// This indicates there is extended data available (currently not used).
    /// (PERF_RECORD_MISC_EXT_RESERVED, since Linux 2.6.35)
    _reserved: bool,
}

impl From<u16> for Metadata {
    fn from(n: u16) -> Self {
        Self {
            _cpu_mode: CpuMode::from(n),
            _multipurpose_lol: (n as u32 | PERF_RECORD_MISC_MMAP_DATA) != 0,
            _exact_ip: (n as u32 | PERF_RECORD_MISC_EXACT_IP) != 0,
            _reserved: (n as u32 | PERF_RECORD_MISC_EXT_RESERVED) != 0,
        }
    }
}

/// The CPU mode can be determined from this value.
enum CpuMode {
    /// Unknown CPU mode. (PERF_RECORD_MISC_CPUMODE_UNKNOWN)
    Unknown,
    /// Sample happened in the kernel. (PERF_RECORD_MISC_KERNEL)
    Kernel,
    /// Sample happened in user code. (PERF_RECORD_MISC_USER)
    User,
    /// Sample happened in the hypervisor. (PERF_RECORD_MISC_HYPERVISOR)
    Hypervisor,
    /// Sample happened in the guest kernel. (PERF_RECORD_MISC_GUEST_KERNEL, since Linux 2.6.35)
    GuestKernel,
    /// Sample happened in guest user code. (PERF_RECORD_MISC_GUEST_USER, since Linux 2.6.35)
    GuestUser,
}

impl From<u16> for CpuMode {
    fn from(n: u16) -> Self {
        match n as u32 | PERF_RECORD_MISC_CPUMODE_MASK {
            PERF_RECORD_MISC_CPUMODE_UNKNOWN => CpuMode::Unknown,
            PERF_RECORD_MISC_KERNEL => CpuMode::Kernel,
            PERF_RECORD_MISC_USER => CpuMode::User,
            PERF_RECORD_MISC_HYPERVISOR => CpuMode::Hypervisor,
            PERF_RECORD_MISC_GUEST_KERNEL => CpuMode::GuestKernel,
            PERF_RECORD_MISC_GUEST_USER => CpuMode::GuestUser,
            other => panic!("unrecognized cpu mode: {}", other),
        }
    }
}

use raw::perf_event_type::*;

enum_from_primitive! {
#[repr(u32)]
pub enum SampledEventType {
    Mmap = PERF_RECORD_MMAP,
    Lost = PERF_RECORD_LOST,
    Comm = PERF_RECORD_COMM,
    Exit = PERF_RECORD_EXIT,
    ThrottleUnthrotte = PERF_RECORD_THROTTLE,
    Fork = PERF_RECORD_FORK,
    Read = PERF_RECORD_READ,
    Sample = PERF_RECORD_SAMPLE,
    Mmap2 = PERF_RECORD_MMAP2,
    Aux = PERF_RECORD_AUX,                       //(since Linux 4.1)
    ItraceStart = PERF_RECORD_ITRACE_START,      //(since Linux 4.1)
    LostSamples = PERF_RECORD_LOST_SAMPLES,      //(since Linux 4.2)
    Switch = PERF_RECORD_SWITCH,                 //(since Linux 4.3)
    SwitchCpuWide = PERF_RECORD_SWITCH_CPU_WIDE, //(since Linux 4.3)
}
}

//    type   The type value is one of the below.  The values in the corre‐
//           sponding record (that follows the header) depend on the type
//           selected as shown.

//           PERF_RECORD_MMAP
//               The MMAP events record the PROT_EXEC mappings so that we
//               can correlate user-space IPs to code.  They have the fol‐
//               lowing structure:

//                   struct {
//                       struct perf_event_header header;
//                       u32    pid, tid;
//                       u64    addr;
//                       u64    len;
//                       u64    pgoff;
//                       char   filename[];
//                   };

//               pid    is the process ID.

//               tid    is the thread ID.

//               addr   is the address of the allocated memory.  len is the
//                      length of the allocated memory.  pgoff is the page
//                      offset of the allocated memory.  filename is a
//                      string describing the backing of the allocated mem‐
//                      ory.

//           PERF_RECORD_LOST
//               This record indicates when events are lost.

//                   struct {
//                       struct perf_event_header header;
//                       u64    id;
//                       u64    lost;
//                       struct sample_id sample_id;
//                   };

//               id     is the unique event ID for the samples that were
//                      lost.

//               lost   is the number of events that were lost.

//           PERF_RECORD_COMM
//               This record indicates a change in the process name.

//                   struct {
//                       struct perf_event_header header;
//                       u32    pid;
//                       u32    tid;
//                       char   comm[];
//                       struct sample_id sample_id;
//                   };

//               pid    is the process ID.

//               tid    is the thread ID.

//               comm   is a string containing the new name of the process.

//           PERF_RECORD_EXIT
//               This record indicates a process exit event.

//                   struct {
//                       struct perf_event_header header;
//                       u32    pid, ppid;
//                       u32    tid, ptid;
//                       u64    time;
//                       struct sample_id sample_id;
//                   };

//           PERF_RECORD_THROTTLE, PERF_RECORD_UNTHROTTLE
//               This record indicates a throttle/unthrottle event.

//                   struct {
//                       struct perf_event_header header;
//                       u64    time;
//                       u64    id;
//                       u64    stream_id;
//                       struct sample_id sample_id;
//                   };

//           PERF_RECORD_FORK
//               This record indicates a fork event.

//                   struct {
//                       struct perf_event_header header;
//                       u32    pid, ppid;
//                       u32    tid, ptid;
//                       u64    time;
//                       struct sample_id sample_id;
//                   };

//           PERF_RECORD_READ
//               This record indicates a read event.

//                   struct {
//                       struct perf_event_header header;
//                       u32    pid, tid;
//                       struct read_format values;
//                       struct sample_id sample_id;
//                   };

//           PERF_RECORD_SAMPLE
//               This record indicates a sample.

//                   struct {
//                       struct perf_event_header header;
//                       u64    sample_id;   /* if PERF_SAMPLE_IDENTIFIER */
//                       u64    ip;          /* if PERF_SAMPLE_IP */
//                       u32    pid, tid;    /* if PERF_SAMPLE_TID */
//                       u64    time;        /* if PERF_SAMPLE_TIME */
//                       u64    addr;        /* if PERF_SAMPLE_ADDR */
//                       u64    id;          /* if PERF_SAMPLE_ID */
//                       u64    stream_id;   /* if PERF_SAMPLE_STREAM_ID */
//                       u32    cpu, res;    /* if PERF_SAMPLE_CPU */
//                       u64    period;      /* if PERF_SAMPLE_PERIOD */
//                       struct read_format v;
//                                           /* if PERF_SAMPLE_READ */
//                       u64    nr;          /* if PERF_SAMPLE_CALLCHAIN */
//                       u64    ips[nr];     /* if PERF_SAMPLE_CALLCHAIN */
//                       u32    size;        /* if PERF_SAMPLE_RAW */
//                       char  data[size];   /* if PERF_SAMPLE_RAW */
//                       u64    bnr;         /* if PERF_SAMPLE_BRANCH_STACK */
//                       struct perf_branch_entry lbr[bnr];
//                                           /* if PERF_SAMPLE_BRANCH_STACK */
//                       u64    abi;         /* if PERF_SAMPLE_REGS_USER */
//                       u64    regs[weight(mask)];
//                                           /* if PERF_SAMPLE_REGS_USER */
//                       u64    size;        /* if PERF_SAMPLE_STACK_USER */
//                       char   data[size];  /* if PERF_SAMPLE_STACK_USER */
//                       u64    dyn_size;    /* if PERF_SAMPLE_STACK_USER &&
//                                              size != 0 */
//                       u64    weight;      /* if PERF_SAMPLE_WEIGHT */
//                       u64    data_src;    /* if PERF_SAMPLE_DATA_SRC */
//                       u64    transaction; /* if PERF_SAMPLE_TRANSACTION */
//                       u64    abi;         /* if PERF_SAMPLE_REGS_INTR */
//                       u64    regs[weight(mask)];
//                                           /* if PERF_SAMPLE_REGS_INTR */
//                   };

//               sample_id
//                   If PERF_SAMPLE_IDENTIFIER is enabled, a 64-bit unique
//                   ID is included.  This is a duplication of the
//                   PERF_SAMPLE_ID id value, but included at the beginning
//                   of the sample so parsers can easily obtain the value.

//               ip  If PERF_SAMPLE_IP is enabled, then a 64-bit instruc‐
//                   tion pointer value is included.

//               pid, tid
//                   If PERF_SAMPLE_TID is enabled, then a 32-bit process
//                   ID and 32-bit thread ID are included.

//               time
//                   If PERF_SAMPLE_TIME is enabled, then a 64-bit time‐
//                   stamp is included.  This is obtained via local_clock()
//                   which is a hardware timestamp if available and the
//                   jiffies value if not.

//               addr
//                   If PERF_SAMPLE_ADDR is enabled, then a 64-bit address
//                   is included.  This is usually the address of a trace‐
//                   point, breakpoint, or software event; otherwise the
//                   value is 0.

//               id  If PERF_SAMPLE_ID is enabled, a 64-bit unique ID is
//                   included.  If the event is a member of an event group,
//                   the group leader ID is returned.  This ID is the same
//                   as the one returned by PERF_FORMAT_ID.

//               stream_id
//                   If PERF_SAMPLE_STREAM_ID is enabled, a 64-bit unique
//                   ID is included.  Unlike PERF_SAMPLE_ID the actual ID
//                   is returned, not the group leader.  This ID is the
//                   same as the one returned by PERF_FORMAT_ID.

//               cpu, res
//                   If PERF_SAMPLE_CPU is enabled, this is a 32-bit value
//                   indicating which CPU was being used, in addition to a
//                   reserved (unused) 32-bit value.

//               period
//                   If PERF_SAMPLE_PERIOD is enabled, a 64-bit value indi‐
//                   cating the current sampling period is written.

//               v   If PERF_SAMPLE_READ is enabled, a structure of type
//                   read_format is included which has values for all
//                   events in the event group.  The values included depend
//                   on the read_format value used at perf_event_open()
//                   time.

//               nr, ips[nr]
//                   If PERF_SAMPLE_CALLCHAIN is enabled, then a 64-bit
//                   number is included which indicates how many following
//                   64-bit instruction pointers will follow.  This is the
//                   current callchain.

//               size, data[size]
//                   If PERF_SAMPLE_RAW is enabled, then a 32-bit value
//                   indicating size is included followed by an array of
//                   8-bit values of length size.  The values are padded
//                   with 0 to have 64-bit alignment.

//                   This RAW record data is opaque with respect to the
//                   ABI.  The ABI doesn't make any promises with respect
//                   to the stability of its content, it may vary depending
//                   on event, hardware, and kernel version.

//               bnr, lbr[bnr]
//                   If PERF_SAMPLE_BRANCH_STACK is enabled, then a 64-bit
//                   value indicating the number of records is included,
//                   followed by bnr perf_branch_entry structures which
//                   each include the fields:

//                   from   This indicates the source instruction (may not
//                          be a branch).

//                   to     The branch target.

//                   mispred
//                          The branch target was mispredicted.

//                   predicted
//                          The branch target was predicted.

//                   in_tx (since Linux 3.11)
//                          The branch was in a transactional memory trans‐
//                          action.

//                   abort (since Linux 3.11)
//                          The branch was in an aborted transactional mem‐
//                          ory transaction.

//                   cycles (since Linux 4.3)
//                          This reports the number of cycles elapsed since
//                          the previous branch stack update.

//                   The entries are from most to least recent, so the
//                   first entry has the most recent branch.

//                   Support for mispred, predicted, and cycles is
//                   optional; if not supported, those values will be 0.

//                   The type of branches recorded is specified by the
//                   branch_sample_type field.

//               abi, regs[weight(mask)]
//                   If PERF_SAMPLE_REGS_USER is enabled, then the user CPU
//                   registers are recorded.

//                   The abi field is one of PERF_SAMPLE_REGS_ABI_NONE,
//                   PERF_SAMPLE_REGS_ABI_32 or PERF_SAMPLE_REGS_ABI_64.

//                   The regs field is an array of the CPU registers that
//                   were specified by the sample_regs_user attr field.
//                   The number of values is the number of bits set in the
//                   sample_regs_user bit mask.

//               size, data[size], dyn_size
//                   If PERF_SAMPLE_STACK_USER is enabled, then the user
//                   stack is recorded.  This can be used to generate stack
//                   backtraces.  size is the size requested by the user in
//                   sample_stack_user or else the maximum record size.
//                   data is the stack data (a raw dump of the memory
//                   pointed to by the stack pointer at the time of sam‐
//                   pling).  dyn_size is the amount of data actually
//                   dumped (can be less than size).  Note that dyn_size is
//                   omitted if size is 0.

//               weight
//                   If PERF_SAMPLE_WEIGHT is enabled, then a 64-bit value
//                   provided by the hardware is recorded that indicates
//                   how costly the event was.  This allows expensive
//                   events to stand out more clearly in profiles.

//               data_src
//                   If PERF_SAMPLE_DATA_SRC is enabled, then a 64-bit
//                   value is recorded that is made up of the following
//                   fields:

//                   mem_op
//                       Type of opcode, a bitwise combination of:

//                       PERF_MEM_OP_NA          Not available
//                       PERF_MEM_OP_LOAD        Load instruction
//                       PERF_MEM_OP_STORE       Store instruction
//                       PERF_MEM_OP_PFETCH      Prefetch
//                       PERF_MEM_OP_EXEC        Executable code

//                   mem_lvl
//                       Memory hierarchy level hit or miss, a bitwise com‐
//                       bination of the following, shifted left by
//                       PERF_MEM_LVL_SHIFT:

//                       PERF_MEM_LVL_NA         Not available
//                       PERF_MEM_LVL_HIT        Hit
//                       PERF_MEM_LVL_MISS       Miss
//                       PERF_MEM_LVL_L1         Level 1 cache
//                       PERF_MEM_LVL_LFB        Line fill buffer
//                       PERF_MEM_LVL_L2         Level 2 cache
//                       PERF_MEM_LVL_L3         Level 3 cache
//                       PERF_MEM_LVL_LOC_RAM    Local DRAM
//                       PERF_MEM_LVL_REM_RAM1   Remote DRAM 1 hop
//                       PERF_MEM_LVL_REM_RAM2   Remote DRAM 2 hops
//                       PERF_MEM_LVL_REM_CCE1   Remote cache 1 hop
//                       PERF_MEM_LVL_REM_CCE2   Remote cache 2 hops
//                       PERF_MEM_LVL_IO         I/O memory
//                       PERF_MEM_LVL_UNC        Uncached memory

//                   mem_snoop
//                       Snoop mode, a bitwise combination of the follow‐
//                       ing, shifted left by PERF_MEM_SNOOP_SHIFT:

//                       PERF_MEM_SNOOP_NA       Not available
//                       PERF_MEM_SNOOP_NONE     No snoop
//                       PERF_MEM_SNOOP_HIT      Snoop hit
//                       PERF_MEM_SNOOP_MISS     Snoop miss
//                       PERF_MEM_SNOOP_HITM     Snoop hit modified

//                   mem_lock
//                       Lock instruction, a bitwise combination of the
//                       following, shifted left by PERF_MEM_LOCK_SHIFT:

//                       PERF_MEM_LOCK_NA        Not available
//                       PERF_MEM_LOCK_LOCKED    Locked transaction

//                   mem_dtlb
//                       TLB access hit or miss, a bitwise combination of
//                       the following, shifted left by PERF_MEM_TLB_SHIFT:

//                       PERF_MEM_TLB_NA         Not available
//                       PERF_MEM_TLB_HIT        Hit
//                       PERF_MEM_TLB_MISS       Miss
//                       PERF_MEM_TLB_L1         Level 1 TLB
//                       PERF_MEM_TLB_L2         Level 2 TLB
//                       PERF_MEM_TLB_WK         Hardware walker
//                       PERF_MEM_TLB_OS         OS fault handler

//               transaction
//                   If the PERF_SAMPLE_TRANSACTION flag is set, then a
//                   64-bit field is recorded describing the sources of any
//                   transactional memory aborts.

//                   The field is a bitwise combination of the following
//                   values:

//                   PERF_TXN_ELISION
//                          Abort from an elision type transaction (Intel-
//                          CPU-specific).

//                   PERF_TXN_TRANSACTION
//                          Abort from a generic transaction.

//                   PERF_TXN_SYNC
//                          Synchronous abort (related to the reported
//                          instruction).

//                   PERF_TXN_ASYNC
//                          Asynchronous abort (not related to the reported
//                          instruction).

//                   PERF_TXN_RETRY
//                          Retryable abort (retrying the transaction may
//                          have succeeded).

//                   PERF_TXN_CONFLICT
//                          Abort due to memory conflicts with other
//                          threads.

//                   PERF_TXN_CAPACITY_WRITE
//                          Abort due to write capacity overflow.

//                   PERF_TXN_CAPACITY_READ
//                          Abort due to read capacity overflow.

//                   In addition, a user-specified abort code can be
//                   obtained from the high 32 bits of the field by shift‐
//                   ing right by PERF_TXN_ABORT_SHIFT and masking with the
//                   value PERF_TXN_ABORT_MASK.

//               abi, regs[weight(mask)]
//                   If PERF_SAMPLE_REGS_INTR is enabled, then the user CPU
//                   registers are recorded.

//                   The abi field is one of PERF_SAMPLE_REGS_ABI_NONE,
//                   PERF_SAMPLE_REGS_ABI_32, or PERF_SAMPLE_REGS_ABI_64.

//                   The regs field is an array of the CPU registers that
//                   were specified by the sample_regs_intr attr field.
//                   The number of values is the number of bits set in the
//                   sample_regs_intr bit mask.

//           PERF_RECORD_MMAP2
//               This record includes extended information on mmap(2) calls
//               returning executable mappings.  The format is similar to
//               that of the PERF_RECORD_MMAP record, but includes extra
//               values that allow uniquely identifying shared mappings.

//                   struct {
//                       struct perf_event_header header;
//                       u32    pid;
//                       u32    tid;
//                       u64    addr;
//                       u64    len;
//                       u64    pgoff;
//                       u32    maj;
//                       u32    min;
//                       u64    ino;
//                       u64    ino_generation;
//                       u32    prot;
//                       u32    flags;
//                       char   filename[];
//                       struct sample_id sample_id;
//                   };

//               pid    is the process ID.

//               tid    is the thread ID.

//               addr   is the address of the allocated memory.

//               len    is the length of the allocated memory.

//               pgoff  is the page offset of the allocated memory.

//               maj    is the major ID of the underlying device.

//               min    is the minor ID of the underlying device.

//               ino    is the inode number.

//               ino_generation
//                      is the inode generation.

//               prot   is the protection information.

//               flags  is the flags information.

//               filename
//                      is a string describing the backing of the allocated
//                      memory.

//           PERF_RECORD_AUX (since Linux 4.1)

//               This record reports that new data is available in the sep‐
//               arate AUX buffer region.

//                   struct {
//                       struct perf_event_header header;
//                       u64    aux_offset;
//                       u64    aux_size;
//                       u64    flags;
//                       struct sample_id sample_id;
//                   };

//               aux_offset
//                      offset in the AUX mmap region where the new data
//                      begins.

//               aux_size
//                      size of the data made available.

//               flags  describes the AUX update.

//                      PERF_AUX_FLAG_TRUNCATED
//                             if set, then the data returned was truncated
//                             to fit the available buffer size.

//                      PERF_AUX_FLAG_OVERWRITE
//                             if set, then the data returned has overwrit‐
//                             ten previous data.

//           PERF_RECORD_ITRACE_START (since Linux 4.1)

//               This record indicates which process has initiated an
//               instruction trace event, allowing tools to properly corre‐
//               late the instruction addresses in the AUX buffer with the
//               proper executable.

//                   struct {
//                       struct perf_event_header header;
//                       u32    pid;
//                       u32    tid;
//                   };

//               pid    process ID of the thread starting an instruction
//                      trace.

//               tid    thread ID of the thread starting an instruction
//                      trace.

//           PERF_RECORD_LOST_SAMPLES (since Linux 4.2)

//               When using hardware sampling (such as Intel PEBS) this
//               record indicates some number of samples that may have been
//               lost.

//                   struct {
//                       struct perf_event_header header;
//                       u64    lost;
//                       struct sample_id sample_id;
//                   };

//               lost   the number of potentially lost samples.

//           PERF_RECORD_SWITCH (since Linux 4.3)

//               This record indicates a context switch has happened.  The
//               PERF_RECORD_MISC_SWITCH_OUT bit in the misc field indi‐
//               cates whether it was a context switch into or away from
//               the current process.

//                   struct {
//                       struct perf_event_header header;
//                       struct sample_id sample_id;
//                   };

//           PERF_RECORD_SWITCH_CPU_WIDE (since Linux 4.3)

//               As with PERF_RECORD_SWITCH this record indicates a context
//               switch has happened, but it only occurs when sampling in
//               CPU-wide mode and provides additional information on the
//               process being switched to/from.  The
//               PERF_RECORD_MISC_SWITCH_OUT bit in the misc field indi‐
//               cates whether it was a context switch into or away from
//               the current process.

//                   struct {
//                       struct perf_event_header header;
//                       u32 next_prev_pid;
//                       u32 next_prev_tid;
//                       struct sample_id sample_id;
//                   };

//               next_prev_pid
//                      The process ID of the previous (if switching in) or
//                      next (if switching out) process on the CPU.

//               next_prev_tid
//                      The thread ID of the previous (if switching in) or
//                      next (if switching out) thread on the CPU.
