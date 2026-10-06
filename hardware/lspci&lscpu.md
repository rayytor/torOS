rayyan@rayyan:~
$ lspci
00:00.0 Host bridge: Intel Corporation Gemini Lake Host Bridge (rev 06)
00:00.1 Signal processing controller: Intel Corporation Celeron/Pentium Silver Processor Dynamic Platform and Thermal Framework Processor Participant (rev 06)
00:00.3 System peripheral: Intel Corporation Celeron/Pentium Silver Processor Gaussian Mixture Model (rev 06)
00:02.0 VGA compatible controller: Intel Corporation GeminiLake [UHD Graphics 600] (rev 06)
00:0e.0 Audio device: Intel Corporation Celeron/Pentium Silver Processor High Definition Audio (rev 06)
00:0f.0 Communication controller: Intel Corporation Celeron/Pentium Silver Processor Trusted Execution Engine Interface (rev 06)
00:12.0 SATA controller: Intel Corporation Celeron/Pentium Silver Processor SATA Controller (rev 06)
00:13.0 PCI bridge: Intel Corporation Gemini Lake PCI Express Root Port (rev f6)
00:14.0 PCI bridge: Intel Corporation Gemini Lake PCI Express Root Port (rev f6)
00:14.1 PCI bridge: Intel Corporation Gemini Lake PCI Express Root Port (rev f6)
00:15.0 USB controller: Intel Corporation Celeron/Pentium Silver Processor USB 3.0 xHCI Controller (rev 06)
00:17.0 Signal processing controller: Intel Corporation Celeron/Pentium Silver Processor I2C 4 (rev 06)
00:17.3 Signal processing controller: Intel Corporation Celeron/Pentium Silver Processor I2C 7 (rev 06)
00:1c.0 SD Host controller: Intel Corporation Celeron/Pentium Silver Processor SDA Standard Compliant SD Host Controller (rev 06)
00:1f.0 ISA bridge: Intel Corporation Celeron/Pentium Silver Processor LPC Controller (rev 06)
00:1f.1 SMBus: Intel Corporation Celeron/Pentium Silver Processor Gaussian Mixture Model (rev 06)
02:00.0 Unassigned class [ff00]: Realtek Semiconductor Co., Ltd. RTS522A PCI Express Card Reader (rev 01)
03:00.0 Network controller: MEDIATEK Corp. MT7921 802.11ax PCI Express Wireless Network Adapter
rayyan@rayyan:~
$ lscpu
Architecture:                x86_64
  CPU op-mode(s):            32-bit, 64-bit
  Address sizes:             39 bits physical, 48 bits virtual
  Byte Order:                Little Endian
CPU(s):                      2
  On-line CPU(s) list:       0,1
Vendor ID:                   GenuineIntel
  Model name:                Intel(R) Celeron(R) N4020 CPU @ 1.10GHz
    CPU family:              6
    Model:                   122
    Thread(s) per core:      1
    Core(s) per socket:      2
    Socket(s):               1
    Stepping:                8
    CPU(s) scaling MHz:      48%
    CPU max MHz:             2800.0000
    CPU min MHz:             800.0000
    BogoMIPS:                2188.80
    Flags:                   fpu vme de pse tsc msr pae mce cx8 apic sep mtrr pg
                             e mca cmov pat pse36 clflush dts acpi mmx fxsr sse 
                             sse2 ss ht tm pbe syscall nx pdpe1gb rdtscp lm cons
                             tant_tsc art arch_perfmon pebs bts rep_good nopl xt
                             opology nonstop_tsc cpuid aperfmperf tsc_known_freq
                              pni pclmulqdq dtes64 monitor ds_cpl vmx est tm2 ss
                             se3 sdbg cx16 xtpr pdcm sse4_1 sse4_2 x2apic movbe 
                             popcnt tsc_deadline_timer aes xsave rdrand lahf_lm 
                             3dnowprefetch cpuid_fault cat_l2 cdp_l2 ssbd ibrs i
                             bpb stibp ibrs_enhanced tpr_shadow flexpriority ept
                              vpid ept_ad fsgsbase tsc_adjust sgx smep erms mpx 
                             rdt_a rdseed smap clflushopt intel_pt sha_ni xsaveo
                             pt xsavec xgetbv1 xsaves dtherm ida arat pln pts vn
                             mi umip rdpid sgx_lc md_clear arch_capabilities
Virtualization features:     
  Virtualization:            VT-x
Caches (sum of all):         
  L1d:                       48 KiB (2 instances)
  L1i:                       64 KiB (2 instances)
  L2:                        4 MiB (1 instance)
NUMA:                        
  NUMA node(s):              1
  NUMA node0 CPU(s):         0,1
Vulnerabilities:             
  Gather data sampling:      Not affected
  Indirect target selection: Not affected
  Itlb multihit:             Not affected
  L1tf:                      Not affected
  Mds:                       Not affected
  Meltdown:                  Not affected
  Mmio stale data:           Not affected
  Reg file data sampling:    Mitigation; Clear Register File
  Retbleed:                  Not affected
  Spec rstack overflow:      Not affected
  Spec store bypass:         Mitigation; Speculative Store Bypass disabled via p
                             rctl
  Spectre v1:                Mitigation; usercopy/swapgs barriers and __user poi
                             nter sanitization
  Spectre v2:                Mitigation; Enhanced / Automatic IBRS; IBPB conditi
                             onal; PBRSB-eIBRS Not affected; BHI SW loop, KVM SW
                              loop
  Srbds:                     Not affected
  Tsa:                       Not affected
  Tsx async abort:           Not affected
  Vmscape:                   Not affected
rayyan@rayyan:~
$ 