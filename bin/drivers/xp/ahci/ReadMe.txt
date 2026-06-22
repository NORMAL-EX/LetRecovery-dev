

License and copyright information
-----------------------------------

	StorAhci - SATA AHCI Controller
	Copyright (C) 2020 Kai Schtrom

	This file is part of StorAhci.

	StorAhci is free software: you can redistribute it and/or modify
	it under the terms of the GNU General Public License as published by
	the Free Software Foundation, either version 3 of the License, or
	(at your option) any later version.

	StorAhci is distributed in the hope that it will be useful,
	but WITHOUT ANY WARRANTY; without even the implied warranty of
	MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
	GNU General Public License for more details.

	You should have received a copy of the GNU General Public License
	along with StorAhci.  If not, see <http://www.gnu.org/licenses/>.


How to compile StorAhci driver
--------------------------------

- install Windows Driver Kit version 7600.16385.1
- unpack StorAhci.rar and copy src folder to C:\src
- run "make_chk.cmd" to create the checked build or run "make_fre.cmd" to create
  the free build from the directory "C:\src"
- the final driver files can be found in "C:\src\bin"


How we build the StorAhci driver
----------------------------------

We use the source files from the original WDK 8 StorAhci driver sample, which
is freely available from Microsoft. The include files are taken from WDK 8
version 8.59.29757. To compile the Windows 2003 compatible driver we use DDK
version 7600.16385.1. Our intention was to apply the smallest possible changes
to the original code to prevent bugs. We have changed the following things to
make the driver work for Windows 2003 Server:

- added INF and txtsetup.oem file to install the driver
- added makefile and sources file to compile the driver
- changed resource file
- added StorPortPatch header and code file with a few necessary StorPort
  functions and replaced the function StorPortExtendedFunction
- changed very important code sections like follows:
  - size of HW_INITIALIZATION_DATA corrected to comply with storport.sys
  - blocked read and write access beyond the end of the structure
    PORT_CONFIGURATION_INFORMATION
  - setup physical address for ReadLogExtPageDataPhysicalAddress correctly
- PREfast and SAL (source code annotation language) changes to make the code
  compatible with the DDK version 7600.16385.1
- fixed PREfast and SAL warnings and errors


Which operating systems are supported?
----------------------------------------

StorAhci is build for Windows 2003 Server with Service Pack 1 and Service Pack 2
in both the x86 and the x64 editions. The driver does not work on Windows 2003
Server without Service Pack, because StorPortNotification inside storport.sys
does not support the needed notification types. The driver was only tested on
Windows 2003 Server with Service Pack 1 and 2, because starting with Windows
Vista Microsoft includes a SATA AHCI driver on the installation media.

