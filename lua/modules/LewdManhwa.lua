----------------------------------------------------------------------------------------------------
-- Local Constants
----------------------------------------------------------------------------------------------------

DirectoryPagination = '/webtoons/page/'

----------------------------------------------------------------------------------------------------
-- Event Functions
----------------------------------------------------------------------------------------------------

-- Get info and chapter list for current manga.
function GetInfo()
	local n, v, x = nil
	local u = MaybeFillHost(MODULE.RootURL, URL)

	if not HTTP.GET(u) then return net_problem end

	x = CreateTXQuery(HTTP.Document)
	MANGAINFO.Title     = x.XPathString('//*[@class="entry-title"]')
	MANGAINFO.CoverLink = x.XPathString('//*[@class="post-thumbnail"]/img/@data-src')
	MANGAINFO.Authors   = x.XPathString('//*[@class="author"]//a')
	MANGAINFO.Genres    = x.XPathStringAll('//*[@class="tags"]/a')
	MANGAINFO.Summary   = x.XPathString('//*[contains(@class, "entry-content")]')

	v = x.XPath('//*[@class="chapter-list-items"]/a')
	n = x.XPath('//*[@class="chapter-name"]')
	for i = 1, v.Count do
		MANGAINFO.ChapterNames.Add(n.Get(i).ToString())
		MANGAINFO.ChapterLinks.Add(v.Get(i).GetAttribute('href'))
	end

	return no_error
end

-- Get the page count of the manga list of the current website.
function GetDirectoryPageNumber()
	local u = MODULE.RootURL .. DirectoryPagination .. 1

	if not HTTP.GET(u) then return net_problem end

	PAGENUMBER = tonumber(CreateTXQuery(HTTP.Document).XPathString('(//span[@class="pages"])/substring-after(., "Page 1 of ")'))

	return no_error
end

-- Get LINKS and NAMES from the manga list of the current website.
function GetNameAndLink()
	local n, v, x = nil
	local u = MODULE.RootURL .. DirectoryPagination .. (URL + 1)

	if not HTTP.GET(u) then return net_problem end

	x = CreateTXQuery(HTTP.Document)
	v = x.XPath('//*[contains(@rel, "bookmark")]')
	n = x.XPath('//*[contains(@class, "entry-title")]')
	for i = 1, v.Count do
		LINKS.Add(v.Get(i).GetAttribute('href'))
		NAMES.Add(n.Get(i).ToString())
	end

	return no_error
end

-- Get the page count for the current chapter.
function GetPageNumber()
	local x = nil
	local u = MaybeFillHost(MODULE.RootURL, URL)

	if not HTTP.GET(u) then return net_problem end

	x = CreateTXQuery(HTTP.Document)
	x.XPathStringAll('//*[@class="container"]//img/@src', TASK.PageLinks)

	return no_error
end

----------------------------------------------------------------------------------------------------
-- Module Initialization
----------------------------------------------------------------------------------------------------

function Init()
	local m = NewWebsiteModule()
	m.ID                       = 'eb03646592fa4718bebb074af764fcdc'
	m.Name                     = 'LewdManhwa'
	m.RootURL                  = 'https://lewdmanhwa.com'
	m.Category                 = 'H-Sites'
	m.OnGetInfo                = 'GetInfo'
	m.OnGetDirectoryPageNumber = 'GetDirectoryPageNumber'
	m.OnGetNameAndLink         = 'GetNameAndLink'
	m.OnGetPageNumber          = 'GetPageNumber'
end
